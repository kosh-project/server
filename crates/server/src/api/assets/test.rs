#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;
use crate::{
    api::Error::BadRequest,
    app::AppStateBuilder,
    auth::Secret,
};
use axum::{
    Extension,
    body::Body,
    extract::{Path, Query, State},
};
use hyper::HeaderMap;
use sqlx::SqlitePool;

async fn setup_state() -> AppState {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("db connect");
    sqlx::migrate!().run(&pool).await.expect("migrate");
    let secret = Secret::random();
    AppStateBuilder::new()
        .vault_path("/tmp")
        .db(pool)
        .secret(secret)
        .build()
}

#[tokio::test]
async fn delete_rejects_invalid_hex_hash() {
    let state = setup_state().await;
    let res = delete(
        State(state),
        Extension(1),
        Path("not_a_valid_hex_hash".to_string()),
    )
    .await;

    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest("Invalid Hash Format")))
    ));
}

#[tokio::test]
async fn get_rejects_invalid_hex_hash() {
    let state = setup_state().await;
    let res = get(
        State(state),
        Extension(1),
        Path("invalid_hex_!!!".to_string()),
    )
    .await;

    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest("Invalid Hash Format")))
    ));
}

#[tokio::test]
async fn upload_rejects_missing_file_name_header() {
    let state = setup_state().await;
    let mut headers = HeaderMap::new();
    headers.insert("Content-Length", "100".parse().unwrap());

    let res = upload(
        State(state),
        Path(asset::Tag::GalleryItem),
        headers,
        Extension(1),
        Body::empty(),
    )
    .await;

    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Missing X-File-Name header"
        )))
    ));
}

#[tokio::test]
async fn upload_rejects_missing_content_length_header() {
    let state = setup_state().await;
    let mut headers = HeaderMap::new();
    headers.insert("X-File-Name", "photo.png".parse().unwrap());

    let res = upload(
        State(state),
        Path(asset::Tag::GalleryItem),
        headers,
        Extension(1),
        Body::empty(),
    )
    .await;

    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Missing content length in header"
        )))
    ));
}

#[tokio::test]
async fn upload_rejects_oversized_payload_exceeding_10gb() {
    let state = setup_state().await;
    let mut headers = HeaderMap::new();
    headers.insert("X-File-Name", "huge.bin".parse().unwrap());
    headers.insert("Content-Length", "10000000001".parse().unwrap()); // 10GB + 1 byte

    let res = upload(
        State(state),
        Path(asset::Tag::GalleryItem),
        headers,
        Extension(1),
        Body::empty(),
    )
    .await;

    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest("Payload too Large")))
    ));
}

#[tokio::test]
async fn list_rejects_partial_cursor() {
    let state = setup_state().await;

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

    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Both before_time and before_id must be provided for cursor pagination"
        )))
    ));
}

#[tokio::test]
async fn list_rejects_invalid_cursor_uuid_format() {
    let state = setup_state().await;

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

    assert!(res.is_err());
    assert!(matches!(
        res,
        Err(crate::Error::ApiError(BadRequest(
            "Invalid before_id cursor format"
        )))
    ));
}
