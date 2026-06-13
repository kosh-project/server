use std::fmt::Display;

pub mod state;
pub use state::AppState;

use axum::{
    Json, Router,
    extract::{Multipart, State},
    response::IntoResponse,
    routing::{get, post},
};
use hyper::StatusCode;
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;

/// Does a debug print
#[inline]
pub fn debug<T, U>(worker: T, action: U)
where
    T: Display,
    U: Display,
{
    eprintln!("\x1b[033m{worker:>12}\x1b[0m -> \x1b[34m{action:<12}\x1b[0m");
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(get_health))
        .route("/storage", get(get_storage))
        .route("/upload", post(handle_upload))
        .with_state(state)
}

async fn get_health() -> Json<Value> {
    debug("HANDLER", "get_health");
    Json(json!({
        "health" : "ok"
    }))
}

async fn handle_upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    debug("HANDLER", "post_upload");

    while let Ok(Some(mut field)) = multipart.next_field().await {
        let file_name = field.file_name().unwrap_or("lmao.dead");

        debug("MULTI_PART", format!("field : {file_name}"));

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

async fn get_storage() -> Json<Value> {
    Json(json!({
        "root" : "/storage",
        "exists" : true,
    }))
}
