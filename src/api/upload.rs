use crate::model::asset::Asset;
use crate::storage::{Error as StorageErr, Payload};
use crate::{api::Error::*, model::asset::AssetTag, storage};
use axum::body::Body;
use axum::{
    Extension, Json,
    extract::{Multipart, Path, State, multipart::Field},
    response::IntoResponse,
};
use hyper::HeaderMap;
use serde::Serialize;

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

    let file_name = headers
        .get("X-File-Name")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            BadRequest("Missing X-File-Name header".into())
        })?;

    let expected_size: u64 = headers
        .get("Content-Length")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.parse().ok())
        .ok_or_else(|| {
            BadRequest("Missing content length in header".into())
        })?;

    if expected_size > 10_000_000_000 {
        return Err(BadRequest("Payload too Large".into()).into());
    }

    let f_stream = body.into_data_stream();

    let status = match state
        .storage
        .try_save(file_name, Payload::new(expected_size, f_stream))
        .await
    {
        Ok(metadata) => {
            match Asset::create(&state.db, user_id, tag, &metadata)
                .await
            {
                Ok(_) => FileStatus::success(
                    file_name.into(),
                    metadata.hash.to_string(),
                ),
                Err(e) => FileStatus::failure(file_name.into(), e),
            }
        }
        Err(e) => FileStatus::failure(file_name.into(), e),
    };

    Ok(Json(status))
}
