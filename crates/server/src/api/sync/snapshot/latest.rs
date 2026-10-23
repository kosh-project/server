use axum::{
    Extension, Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};

use crate::{
    app, error::JsonResult, model::sync::LedgerSnapshot,
    storage::ledger::Namespace,
};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub namespace: Namespace,
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

/// Fetches the latest snapshot metadata for a given user and namespace.
///
/// # Errors
///
/// Returns an error if querying the database fails.
pub async fn latest_snapshot(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(params): Query<Params>,
) -> JsonResult<Option<Response>> {
    let snapshot =
        LedgerSnapshot::latest(&state.db, user_id, params.namespace.as_str())
            .await?;

    Ok(Json(snapshot.map(Response::from)))
}
