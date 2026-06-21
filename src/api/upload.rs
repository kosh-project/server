use crate::{api::Error::*, storage};
use axum::{
    Json,
    extract::{Multipart, State},
    response::IntoResponse,
};
use serde::Serialize;
use serde_json::json;
use tokio::fs::File;

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

    fn failure(file_name: String, error: storage::Error) -> Self {
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

pub(crate) async fn handle_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> crate::Result<impl IntoResponse> {
    log!("HANDLER", "post_upload");

    // let join_set = JoinSet::new();

    let mut file_stats = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|_| StreamReadError)? {
        let file_name = field.file_name().unwrap_or("unnamed_file").to_string();
        log!("MULTI_PART", format!("field : {file_name}"));

        let result = state.storage.try_save(&file_name, field).await;

        match result {
            Ok(hash) => file_stats.push(FileStatus::success(file_name, hash)),
            Err(error) => file_stats.push(FileStatus::failure(file_name, error)),
        }
    }
    Ok(Json(UploadResponse::from(file_stats)))
}
