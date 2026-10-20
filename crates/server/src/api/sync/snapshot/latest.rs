use axum::{
    Extension, Json,
    extract::{self, Query, State},
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use crate::{Result, app, model::sync::LedgerSnapshot};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub namespace: String,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub id: i64,
    pub snapshot_seq: i64,
    pub blob_hash: String,
    pub created_at: i64,
}

impl From<LedgerSnapshot> for Response {
    fn from(snap: LedgerSnapshot) -> Self {
        Self {
            id: snap.id,
            snapshot_seq: snap.snapshot_seq,
            blob_hash: hex::encode(snap.blob_hash),
            created_at: snap.created_at,
        }
    }
}

pub async fn latest_snapshot(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(params): Query<Params>,
) -> Result<impl IntoResponse> {
    let snapshot =
        LedgerSnapshot::latest(&state.db, user_id, &params.namespace).await?;

    Ok(Json(snapshot.map(Response::from)))
}
