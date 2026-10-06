#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::as_conversions)]

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
use tmpdir::TmpDir;

use crate::{
    api::{
        Error,
        sync::{
            PruneRequest, SyncRequest, append_delta, prune_ledger, stream_delta,
        },
    },
    app::{self, AppStateBuilder},
    auth::Secret,
};

async fn setup_env() -> Result<(TmpDir, app::State)> {
    let tmp = TmpDir::new("api_sync").await?;
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    let secret = Secret::new(rand::random());

    let state = AppStateBuilder::new()
        .vault_path(tmp.to_path_buf())
        .db(pool)
        .secret(secret)
        .build();

    Ok((tmp, state))
}

#[tokio::test]
async fn api_delta_append_success() -> anyhow::Result<()> {
    let (_tmp, state) = setup_env().await?;

    let user_id = 99;
    let payload = Bytes::from("PAYLOAD PAYLOAD");

    let result =
        append_delta(State(state), Extension(user_id), payload.clone()).await?;

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

    let response = stream_delta(State(state.clone()), Extension(user_id), query)
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
