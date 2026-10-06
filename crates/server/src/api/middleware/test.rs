#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::{
    auth_guard::auth_guard, mac_guard::mac_guard, rate_limit::TokenExtractor,
};
use crate::{
    app::AppStateBuilder,
    auth::Secret,
    model::session::{Session, TokenHash},
};
use anyhow::Result;
use axum::{
    Extension, Router,
    body::Body,
    extract::Request,
    http::StatusCode,
    routing::get,
};
use blake3::hash;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;
use tower_governor::key_extractor::KeyExtractor;

async fn setup_state() -> Result<(crate::app::State, Secret)> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await?;
    sqlx::migrate!().run(&pool).await?;

    let secret = Secret::random();
    let state = AppStateBuilder::new()
        .vault_path("/tmp")
        .db(pool)
        .secret(secret)
        .build();

    Ok((state, secret))
}

#[tokio::test]
async fn auth_guard_bypasses_db_on_cache_hit() -> Result<()> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await?;
    let secret = Secret::random();

    let state = AppStateBuilder::new()
        .vault_path("/tmp")
        .db(pool)
        .secret(secret)
        .build();

    let token = "top_secret";
    let token_hash = TokenHash::from(hash(token.as_bytes()).as_bytes());

    state.session_cache.insert(token_hash, 4).await;

    let app = Router::new()
        .route("/", get(|| async { "Success!" }))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth_guard,
        ))
        .with_state(state);

    let mut req = Request::builder()
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    req.extensions_mut().insert(token_hash);

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test]
async fn mac_guard_missing_header_returns_unauthorized() -> Result<()> {
    let (state, _) = setup_state().await?;

    let app = Router::new()
        .route("/protected", get(|| async { "OK" }))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            mac_guard,
        ))
        .with_state(state);

    let req = Request::builder()
        .uri("/protected")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}

#[tokio::test]
async fn mac_guard_non_bearer_header_returns_unauthorized() -> Result<()> {
    let (state, _) = setup_state().await?;

    let app = Router::new()
        .route("/protected", get(|| async { "OK" }))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            mac_guard,
        ))
        .with_state(state);

    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", "Basic dXNlcjpwYXNz")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}

#[tokio::test]
async fn mac_guard_forged_token_rejected_in_ram() -> Result<()> {
    let (state, _) = setup_state().await?;

    let app = Router::new()
        .route("/protected", get(|| async { "OK" }))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            mac_guard,
        ))
        .with_state(state);

    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", "Bearer forged.invalid.token")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    Ok(())
}

#[tokio::test]
async fn mac_guard_valid_token_passes_and_injects_hash() -> Result<()> {
    let (state, secret) = setup_state().await?;

    // Create user and token
    sqlx::query!(
        "INSERT INTO users (id, identity_hash, auth_verifier) VALUES (1, ?, 'verifier')",
        &[0u8; 32][..]
    )
    .execute(&state.db)
    .await?;

    let token = Session::create(&state.db, 1, &secret).await?;

    let app = Router::new()
        .route(
            "/protected",
            get(|Extension(token_hash): Extension<TokenHash>| async move {
                format!("{:?}", token_hash.0)
            }),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            mac_guard,
        ))
        .with_state(state);

    let req = Request::builder()
        .uri("/protected")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    Ok(())
}

#[test]
fn token_extractor_key_extraction() {
    let extractor = TokenExtractor;
    let token_hash = TokenHash([5u8; 32]);

    // Request with TokenHash extension
    let mut req = Request::builder().body(()).unwrap();
    req.extensions_mut().insert(token_hash);

    let extracted = extractor.extract(&req);
    assert!(extracted.is_ok());
    assert_eq!(extracted.unwrap(), token_hash);

    // Request without TokenHash extension
    let empty_req = Request::builder().body(()).unwrap();
    let failed = extractor.extract(&empty_req);
    assert!(failed.is_err());
}
