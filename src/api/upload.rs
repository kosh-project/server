use std::fs::File;

use crate::model::asset::Asset;
use crate::storage::Error as StorageErr;
use crate::{api::Error::*, model::asset::AssetTag, storage};
use axum::{
    Json,
    extract::{Multipart, State, multipart::Field},
    response::IntoResponse,
};
use serde::Serialize;
use sqlx::query;

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

#[derive(Serialize)]
struct UploadResponse {
    stats: Vec<FileStatus>,
}

impl From<Vec<FileStatus>> for UploadResponse {
    fn from(stats: Vec<FileStatus>) -> Self {
        Self { stats }
    }
}

pub async fn upload(
    State(state): State<AppState>,
    axum::Extension(user_id): axum::Extension<i64>,
    mut multipart: Multipart,
) -> crate::Result<impl IntoResponse> {
    log!("HANDLER", "post_upload");

    let mut current_tag: Option<AssetTag> = None;
    let mut file_stats = Vec::new();

    while let Some(field) =
        multipart.next_field().await.map_err(|_| StreamReadError)?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "tag" {
            let tag_str = field.text().await.map_err(|_| {
                BadRequest("Corrupted Tag field".into())
            })?;

            current_tag =
                Some(AssetTag::try_from(tag_str.as_str()).map_err(
                    |_| BadRequest("Invalid tag integer".into()),
                )?);
            continue;
        }

        if field_name == "file" {
            let tag = current_tag.ok_or_else(|| {
                BadRequest("Expected 'tag' field, found 'file'".into())
            })?;

            let status =
                process_file(&state, user_id, tag, field).await;
            file_stats.push(status);
        }
    }

    Ok(Json(UploadResponse::from(file_stats)))
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
