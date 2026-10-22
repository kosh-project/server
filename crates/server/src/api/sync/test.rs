#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::as_conversions)]

use anyhow::{Context, Result};
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
        sync::{append_delta, prune, prune_ledger, pull, push, stream_delta},
    },
    app::{self, AppStateBuilder},
    auth::Secret,
    storage::ledger::Namespace,
};

async fn setup_env() -> Result<(TmpDir, app::State)> {
    let tmp = TmpDir::new("api_sync")
        .await
        .context("failed to create temporary directory")?;
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    sqlx::migrate!().run(&pool).await.context("migrate")?;
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

    let result = append_delta(
        State(state),
        Extension(user_id),
        Query(push::Params {
            namespace: Namespace::Files,
        }),
        payload.clone(),
    )
    .await
    .context("failed to append delta")?;

    let reciept = result.0;

    // Test: verify ledger delta append returns file name and calculated byte offset
    assert_eq!(reciept.file_name, "current.log");
    assert_eq!(reciept.sequence_id, 1);
    assert_eq!(reciept.offset, 500 + 4 + 8 + payload.len() as u64);

    Ok(())
}

#[tokio::test]
async fn api_stream_success() -> anyhow::Result<()> {
    let (_tmp, state) = setup_env().await?;
    let user_id = 99;

    let _ = append_delta(
        State(state.clone()),
        Extension(user_id),
        Query(push::Params {
            namespace: Namespace::Files,
        }),
        Bytes::from("PAYLOAD"),
    )
    .await
    .context("failed to append delta for streaming")?;

    let query = Query(pull::Params {
        namespace: Namespace::Files,
        since_seq: None,
        offset: Some(0),
        limit: None,
        file: Some("current.log".into()),
    });

    let response =
        stream_delta(State(state.clone()), Extension(user_id), query)
            .await
            .context("failed to stream delta")?
            .into_response();

    // Test: verify successful stream response status is 200 OK
    assert_eq!(response.status(), 200);

    let body_bytes = response
        .into_body()
        .collect()
        .await
        .context("failed to collect streamed response body")?
        .to_bytes();
    let len = u32::from_be_bytes(
        body_bytes[0..4]
            .try_into()
            .context("failed to parse streamed delta length header")?,
    );
    let seq = u64::from_be_bytes(
        body_bytes[4..12]
            .try_into()
            .context("failed to parse streamed delta sequence ID")?,
    );

    // Test: verify streamed payload length, sequence, and content matches appended delta
    assert_eq!(len, 7);
    assert_eq!(seq, 1);
    assert_eq!(&body_bytes[12..], b"PAYLOAD");

    Ok(())
}

#[tokio::test]
async fn api_stream_path_traversal() -> anyhow::Result<()> {
    let (_tmp, state) = setup_env().await?;
    let user_id = 99;

    let query = Query(pull::Params {
        namespace: Namespace::Files,
        file: Some("../../../etc/passwd".to_string()),
        since_seq: None,
        offset: Some(0),
        limit: None,
    });

    let res = stream_delta(State(state), Extension(user_id), query).await;

    // Test: verify directory traversal attempt in stream delta filename returns BadRequest
    assert!(matches!(res, Err(Error::BadRequest(_))));

    Ok(())
}

#[tokio::test]
async fn api_delta_not_found() -> anyhow::Result<()> {
    let (_tmp, state) = setup_env().await?;
    let user_id = 99;

    let query = Query(pull::Params {
        namespace: Namespace::Files,
        file: Some("0000000000000099.log".to_string()),
        since_seq: None,
        offset: Some(500),
        limit: None,
    });

    let res = stream_delta(State(state), Extension(user_id), query).await;

    let response = res.into_response();
    // Test: verify requesting nonexistent delta file returns 404 NOT_FOUND status
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[tokio::test]
async fn api_delta_prune_success() -> anyhow::Result<()> {
    let (_tmp, state) = setup_env().await?;
    let user_id = 99;

    let query = Query(prune::Params {
        namespace: Namespace::Gallery,
    });

    let res = prune_ledger(State(state), Extension(user_id), query)
        .await
        .context("failed to prune ledger")?;

    let response = res.into_response();
    // Test: verify pruning ledgers returns 200 OK status
    assert_eq!(response.status(), StatusCode::OK);
    Ok(())
}
