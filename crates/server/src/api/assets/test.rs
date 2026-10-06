#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_wraps)]

use super::*;
use crate::{api::Error::BadRequest, app::AppStateBuilder, auth::Secret};
use anyhow::{Context, Result};
use axum::{
    Extension,
    body::Body,
    extract::{Path, Query, State},
    http::HeaderValue,
};
use hyper::HeaderMap;
use sqlx::SqlitePool;

async fn setup_state() -> Result<AppState> {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .context("db connect")?;
    sqlx::migrate!().run(&pool).await.context("migrate")?;
    let secret = Secret::random();
    Ok(AppStateBuilder::new()
        .vault_path("/tmp")
        .db(pool)
        .secret(secret)
        .build())
}

#[tokio::test]
async fn delete_rejects_invalid_hex_hash() -> Result<()> {
    let state = setup_state().await.context("failed to setup app state")?;
    let res = delete(
        State(state),
        Extension(1),
        Path("not_a_valid_hex_hash".to_string()),
    )
    .await;

    // Test: verify delete rejects non-hexadecimal hash format
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest("Invalid Hash Format")))
    ));
    Ok(())
}

#[tokio::test]
async fn get_rejects_invalid_hex_hash() -> Result<()> {
    let state = setup_state().await.context("failed to setup app state")?;
    let res = get(
        State(state),
        Extension(1),
        Path("invalid_hex_!!!".to_string()),
    )
    .await;

    // Test: verify get rejects invalid hex characters in hash parameter
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest("Invalid Hash Format")))
    ));
    Ok(())
}

#[tokio::test]
async fn upload_rejects_missing_file_name_header() -> Result<()> {
    let state = setup_state().await.context("failed to setup app state")?;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Length", HeaderValue::from_static("100"));

    let res = upload(
        State(state),
        Path(asset::Tag::GalleryItem),
        headers,
        Extension(1),
        Body::empty(),
    )
    .await;

    // Test: verify upload returns BadRequest when X-File-Name header is missing
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Missing X-File-Name header"
        )))
    ));
    Ok(())
}

#[tokio::test]
async fn upload_rejects_missing_content_length_header() -> Result<()> {
    let state = setup_state().await.context("failed to setup app state")?;
    let mut headers = HeaderMap::new();
    headers.insert("X-File-Name", HeaderValue::from_static("photo.png"));

    let res = upload(
        State(state),
        Path(asset::Tag::GalleryItem),
        headers,
        Extension(1),
        Body::empty(),
    )
    .await;

    // Test: verify upload returns BadRequest when Content-Length header is omitted
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Missing content length in header"
        )))
    ));
    Ok(())
}

#[tokio::test]
async fn upload_rejects_oversized_payload_exceeding_10gb() -> Result<()> {
    let state = setup_state().await.context("failed to setup app state")?;
    let mut headers = HeaderMap::new();
    headers.insert("X-File-Name", HeaderValue::from_static("huge.bin"));
    headers.insert("Content-Length", HeaderValue::from_static("10000000001")); // 10GB + 1 byte

    let res = upload(
        State(state),
        Path(asset::Tag::GalleryItem),
        headers,
        Extension(1),
        Body::empty(),
    )
    .await;

    // Test: verify upload returns BadRequest when Content-Length exceeds 10GB ceiling
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest("Payload too Large")))
    ));
    Ok(())
}

#[tokio::test]
async fn list_rejects_partial_cursor() -> Result<()> {
    let state = setup_state().await.context("failed to setup app state")?;

    // before_time without before_id
    let res = list(
        State(state.clone()),
        Extension(1),
        Query(ListQuery {
            tag: asset::Tag::GalleryItem,
            limit: Some(10),
            before_time: Some(123_456_789),
            before_id: None,
        }),
    )
    .await;

    // Test: verify cursor pagination rejects before_time when before_id is missing
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Both before_time and before_id must be provided for cursor pagination"
        )))
    ));

    // before_id without before_time
    let res = list(
        State(state),
        Extension(1),
        Query(ListQuery {
            tag: asset::Tag::GalleryItem,
            limit: Some(10),
            before_time: None,
            before_id: Some("0123456789abcdef0123456789abcdef".to_string()),
        }),
    )
    .await;

    // Test: verify cursor pagination rejects before_id when before_time is missing
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Both before_time and before_id must be provided for cursor pagination"
        )))
    ));
    Ok(())
}

#[tokio::test]
async fn list_rejects_invalid_cursor_uuid_format() -> Result<()> {
    let state = setup_state().await.context("failed to setup app state")?;

    let res = list(
        State(state),
        Extension(1),
        Query(ListQuery {
            tag: asset::Tag::GalleryItem,
            limit: Some(10),
            before_time: Some(123_456_789),
            before_id: Some("invalid_not_32_chars".to_string()),
        }),
    )
    .await;

    // Test: verify cursor pagination rejects non-hex/invalid-length before_id
    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Invalid before_id cursor format"
        )))
    ));
    Ok(())
}
