use axum::body::Body;
use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use axum::{Extension, extract::State};
use axum::{Json, body};
use bytes::Bytes;
use hyper::{HeaderMap, StatusCode, header};
use serde::{Deserialize, Serialize};
use tokio_util::io::ReaderStream;

use crate::api::{Error, Result};
use crate::{app, storage::ledger::AppendReciept};

#[derive(Serialize)]
pub struct AppendResponse {
    pub file_name: String,
    pub offset: u64,
}

impl From<AppendReciept> for AppendResponse {
    fn from(reciept: AppendReciept) -> Self {
        Self {
            file_name: reciept.file_name,
            offset: reciept.offset,
        }
    }
}

pub async fn append_delta(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    body: Bytes,
) -> Result<Json<AppendResponse>> {
    let len = body.len() as u32;

    let mut payload = Vec::with_capacity(2 + body.len());
    payload.extend_from_slice(&len.to_le_bytes());
    payload.extend_from_slice(&body);

    let receipt = state.ledger.append(user_id, Bytes::from(payload)).await?;

    Ok(Json(receipt.into()))
}

#[derive(Deserialize)]
pub struct SyncRequest {
    pub file: String,
    pub offset: u64,
}

use crate::api::Error::InvalidHeader;

pub async fn stream_delta(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(query): Query<SyncRequest>,
) -> Result<impl IntoResponse> {
    if query.file.contains('\\')
        || query.file.contains('/')
        || !query.file.starts_with("delta_")
    {
        return Err(Error::BadRequest("Invalid file name".into()));
    }

    let file = state
        .ledger
        .read_segment(state.vault_path(), user_id, &query.file, query.offset)
        .await?;

    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type",
        "application/octet-stream".parse().map_err(InvalidHeader)?,
    );

    Ok((StatusCode::OK, headers, body))
}

#[derive(Deserialize)]
pub struct PruneRequest {
    pub before: u32,
}

pub async fn prune_ledger(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(query): Query<PruneRequest>,
) -> Result<impl IntoResponse> {
    state.ledger.prune(user_id, query.before).await?;

    Ok(StatusCode::OK)
}
