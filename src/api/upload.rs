use axum::{Json, extract::{Multipart, State}, response::IntoResponse};
use hyper::StatusCode;
use serde_json::json;
use tokio::io::AsyncWriteExt;

use crate::{AppState, log};

pub(crate) async fn handle_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    log!("HANDLER", "post_upload");

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let file_name = field.file_name().unwrap_or("lmao.dead");

        log!("MULTI_PART", format!("field : {file_name}"));

        let file_path = state.vault_path().join(file_name);

        let mut file = match tokio::fs::File::create(&file_path).await {
            Ok(f) => f,
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Disk IO Error {e}"),
                ));
            }
        };

        while let Ok(Some(chunk)) = field.chunk().await {
            file.write_all(&chunk).await.map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to write to file : {e}"),
                )
            })?;
        }
    }

    Ok(Json(json! ({
        "diddy_do_it?" : true
    })))
}