use axum::{
    Extension,
    body::Body,
    extract::{Query, State},
    response::IntoResponse,
};
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use tokio_util::io::ReaderStream;

use crate::api::Result;
use crate::{
    api::Error::{BadRequest, InvalidHeader},
    app::{self},
};

#[derive(Debug, Deserialize)]
pub struct Params {
    pub namespace: String,
    pub file: Option<String>,
    pub since_seq: Option<u64>,
    pub offset: Option<u64>,
    pub limit: Option<u32>,
}

pub async fn stream_delta(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Query(query): Query<Params>,
) -> Result<impl IntoResponse> {
    let file_name = query.file.as_deref().unwrap_or("");
    if file_name.contains('\\')
        || file_name.contains('/')
        || !file_name.starts_with("delta_")
    {
        return Err(BadRequest("Invalid file name"));
    }

    let offset = query.offset.unwrap_or(0);
    let file = state
        .ledger
        .read_segment(state.vault_path(), user_id, file_name, offset)
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
