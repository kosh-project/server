use axum::{Json, Router, routing::{get, post}};
use serde_json::{Value, json};

use crate::{AppState, api::upload::handle_upload, log};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(get_health))
        .route("/storage", get(get_storage))
        .route("/upload", post(handle_upload))
        .with_state(state)
}

async fn get_health() -> Json<Value> {
    log!("HANDLER", "get_health");
    Json(json!({
        "health" : "ok"
    }))
}


async fn get_storage() -> Json<Value> {
    Json(json!({
        "root" : "/storage",
        "exists" : true,
    }))
}