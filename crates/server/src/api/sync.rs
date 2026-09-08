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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use axum::{
        Extension,
        extract::{Query, State},
        response::IntoResponse,
    };
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use hyper::StatusCode;
    use sqlx::SqlitePool;
    use std::{any, assert_matches};
    use tmpdir::TmpDir;

    use crate::{
        api::{
            Error,
            sync::{
                PruneRequest, SyncRequest, append_delta, prune_ledger,
                stream_delta,
            },
        },
        app::{self, AppStateBuilder},
    };

    async fn setup_env() -> Result<(TmpDir, app::State)> {
        let tmp = TmpDir::new("api_sync").await?;

        let pool = SqlitePool::connect("sqlite::memory:").await?;

        let state = AppStateBuilder::new()
            .vault_path(tmp.to_path_buf())
            .db(pool)
            .build();

        Ok((tmp, state))
    }

    #[tokio::test]
    async fn api_delta_append_success() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;

        let user_id = 99;
        let payload = Bytes::from("PAYLOAD PAYLOAD");

        let result =
            append_delta(State(state), Extension(user_id), payload.clone())
                .await?;

        let reciept = result.0;

        assert_eq!(reciept.file_name, "delta_0000001");
        assert_eq!(reciept.offset, 500 + 4 + payload.len() as u64);

        Ok(())
    }

    #[tokio::test]
    async fn api_stream_success() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let _ = append_delta(
            State(state.clone()),
            Extension(user_id),
            Bytes::from("PAYLOAD"),
        )
        .await?;

        let query = Query(SyncRequest {
            file: "delta_0000001".into(),
            offset: 0,
        });

        let response =
            stream_delta(State(state.clone()), Extension(user_id), query)
                .await?
                .into_response();

        assert_eq!(response.status(), 200);

        let body_bytes = response.into_body().collect().await?.to_bytes();
        let len = u32::from_le_bytes(body_bytes[0..4].try_into()?);

        assert_eq!(len, 7);
        assert_eq!(&body_bytes[4..], b"PAYLOAD");

        Ok(())
    }

    #[tokio::test]
    async fn api_stream_path_traversal() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let query = Query(SyncRequest {
            file: "../../../etc/passwd".to_string(),
            offset: 0,
        });

        let res = stream_delta(State(state), Extension(user_id), query).await;

        assert!(matches!(res, Err(Error::BadRequest(_))));

        Ok(())
    }

    #[tokio::test]
    async fn api_delta_not_found() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let query = Query(SyncRequest {
            file: "delta_00000099".to_string(),
            offset: 500,
        });

        let res = stream_delta(State(state), Extension(user_id), query).await;

        let response = res.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        Ok(())
    }

    #[tokio::test]
    async fn api_delta_prune_success() -> anyhow::Result<()> {
        let (_tmp, state) = setup_env().await?;
        let user_id = 99;

        let query = Query(PruneRequest { before: 1 });

        let res = prune_ledger(State(state), Extension(user_id), query).await?;

        let response = res.into_response();

        assert_eq!(response.status(), StatusCode::OK);
        Ok(())
    }
}
