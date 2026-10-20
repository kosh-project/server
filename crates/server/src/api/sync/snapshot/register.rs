use crate::{Result, app, model::sync::LedgerSnapshot};
use axum::{Extension, Json, extract::State, response::IntoResponse};
use chrono::Utc;
use serde::Deserialize;

use crate::api::Error::BadRequest;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub namespace: String,
    pub snapshot_seq: i64,
    pub blob_hash: String,
}

pub async fn register_snapshot(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Json(payload): Json<Request>,
) -> Result<impl IntoResponse> {
    let hash_bytes = hex::decode(&payload.blob_hash)
        .map_err(|_| BadRequest("Invalid hex blob_hash"))?;

    if hash_bytes.len() != 32 {
        Err(BadRequest("blob_hash must be 32 bytes in length"))?;
    }

    let now = Utc::now().timestamp();
    let id = LedgerSnapshot::create(
        &state.db,
        user_id,
        &payload.namespace,
        payload.snapshot_seq,
        &hash_bytes,
        now,
    )
    .await?;

    Ok(Json(id))
}
