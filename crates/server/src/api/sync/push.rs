use axum::{
    Extension, Json,
    extract::{Query, State},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::{
    api::Result,
    app,
    storage::ledger::{AppendReceipt, Namespace},
};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub namespace: Namespace,
}

#[derive(Serialize, Debug)]
pub struct Response {
    pub sequence_id: u64,
    pub file_name: String,
    pub offset: u64,
}

impl From<AppendReceipt> for Response {
    fn from(receipt: AppendReceipt) -> Self {
        Self {
            sequence_id: receipt.sequence_id,
            file_name: receipt.file_name,
            offset: receipt.offset,
        }
    }
}

/// Appends an encrypted delta payload to the active segment for the namespace.
///
/// # Errors
///
/// Returns an error if writing to disk or communicating with the committer fails.
pub async fn append_delta(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(params): Query<Params>,
    body: Bytes,
) -> Result<Json<Response>> {
    let receipt = state.ledger.append(user_id, params.namespace, body).await?;

    Ok(Json(receipt.into()))
}
