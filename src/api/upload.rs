use crate::model::asset::Asset;
use crate::storage::Error as StorageErr;
use crate::{api::Error::*, model::asset::AssetTag, storage};
use axum::body::Body;
use axum::{
    Extension, Json,
    extract::{Multipart, Path, State, multipart::Field},
    response::IntoResponse,
};
use hyper::HeaderMap;
use serde::Serialize;
use sqlx::query;
use std::fs::File;

use crate::{app::State as AppState, log};

#[derive(Serialize)]
#[serde(untagged)]
enum FileStatus {
    Success { file_name: String, hash: String },
    Failure { file_name: String, error: String },
}

impl FileStatus {
    fn success(file_name: String, hash: String) -> Self {
        Self::Success { file_name, hash }
    }

    fn failure(file_name: String, error: impl ToString) -> Self {
        Self::Failure {
            file_name,
            error: error.to_string(),
        }
    }
}

pub async fn upload(
    State(state): State<AppState>,
    Path(tag_str): Path<String>,
    headers: HeaderMap,
    Extension(user_id): Extension<i64>,
    body: Body,
) -> crate::Result<impl IntoResponse> {
    log!("HANDLER", "post_upload");

    let tag = AssetTag::try_from(tag_str.as_str())
        .map_err(|_| BadRequest("Invalid Tag".into()))?;

    let file_name = headers.get("X-File-Name")
    .and_then(|v| v.to_str().ok())
    .ok_or_else(|| BadRequest("Missing X-File-Name header".into()))?;

    let f_stream = body.into_data_stream();

    let status = match state.storage.try_save(file_name, f_stream).await {
        Ok(metadata) => {
            match Asset::create(&state.db, user_id, tag, &metadata).await {
                Ok(_) => FileStatus::success(file_name.into(), metadata.hash.to_string()),
                Err(e) => FileStatus::failure(file_name.into(), e)
            }
        },
        Err(e) => FileStatus::failure(file_name.into(), e)
    };

    Ok(Json(status))
}

async fn process_file(
    state: &AppState,
    user_id: i64,
    tag: AssetTag,
    field: Field<'_>,
) -> FileStatus {
    let file_name = match field.file_name() {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => {
            return FileStatus::failure(
                "UNKNOWN".to_string(),
                "Missing filename",
            );
        }
    };

    let metadata = match state.storage.try_save(&file_name, field).await
    {
        Ok(m) => m,
        Err(e) => return FileStatus::failure(file_name, e),
    };

    match Asset::create(&state.db, user_id, tag, &metadata).await {
        Ok(_) => FileStatus::success(
            file_name,
            metadata.hash.to_hex().to_string(),
        ),
        Err(e) => FileStatus::failure(file_name, e),
    }
}
