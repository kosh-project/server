use axum::{
    Extension, Json,
    extract::{Query, State},
    response::IntoResponse,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    Result, app, model::sync::DeviceWatermark, storage::ledger::Namespace,
};

const STALE_DEVICE_THRESHOLD_SECS: i64 = 30 * 24 * 60 * 60; // 30 days

#[derive(Debug, Deserialize)]
pub struct Params {
    pub namespace: Namespace,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub safe_seq: u64,
    pub pruned_segments: usize,
}

/// Prunes sealed delta segments for the authenticated user and namespace.
///
/// # Errors
///
/// Returns an error if querying device watermarks or unlinking segment files fails.
pub async fn prune_ledger(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(params): Query<Params>,
) -> Result<impl IntoResponse> {
    let stale_cutoff = Utc::now()
        .timestamp()
        .saturating_sub(STALE_DEVICE_THRESHOLD_SECS);

    let safe_seq_opt = DeviceWatermark::safe_prune_seq(
        &state.db,
        user_id,
        params.namespace.as_str(),
        stale_cutoff,
    )
    .await?;

    let safe_seq = match safe_seq_opt {
        Some(seq) if seq > 0 => u64::try_from(seq).unwrap_or(0),
        _ => {
            return Ok(Json(Response {
                safe_seq: 0,
                pruned_segments: 0,
            }));
        }
    };

    let pruned_segments = state
        .ledger
        .prune(user_id, params.namespace, safe_seq)
        .await?;

    Ok(Json(Response {
        safe_seq,
        pruned_segments,
    }))
}
