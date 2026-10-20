use crate::api::Result;
use axum::{Extension, Json, extract::State};
use bytes::Bytes;
use serde::Serialize;

use crate::{app, storage::ledger::AppendReceipt};

#[derive(Serialize, Debug)]
pub struct Response {
    pub file_name: String,
    pub offset: u64,
}

impl From<AppendReceipt> for Response {
    fn from(receipt: AppendReceipt) -> Self {
        Self {
            file_name: receipt.file_name,
            offset: receipt.offset,
        }
    }
}

pub async fn append_delta(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    body: Bytes,
) -> Result<Json<Response>> {
    let len = u32::try_from(body.len())?;

    let mut payload = Vec::with_capacity(4 + body.len());
    payload.extend_from_slice(&len.to_le_bytes());
    payload.extend_from_slice(&body);

    let receipt = state.ledger.append(user_id, Bytes::from(payload)).await?;

    Ok(Json(receipt.into()))
}
