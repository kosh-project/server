#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_wraps)]

use super::{
    auth_guard::auth_guard, mac_guard::mac_guard, rate_limit::TokenExtractor,
};
use crate::{
    app::AppStateBuilder,
    auth::Secret,
    model::session::{Session, TokenHash},
};
use anyhow::{Context, Result};
use axum::{
    Extension, Router, body::Body, extract::Request, http::StatusCode,
    routing::get,
};
use blake3::hash;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;
use tower_governor::key_extractor::KeyExtractor;

async fn setup_state() -> Result<(crate::app::State, Secret)> {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .context("db connect")?;
    sqlx::migrate!().run(&pool).await.context("migrate")?;

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
        .await
        .context("db connect")?;
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
        .context("failed to build request")?;

    req.extensions_mut().insert(token_hash);

    let response = app
        .oneshot(req)
        .await
        .context("failed to process request")?;
    // Test: verify auth guard bypasses DB lookup and returns 200 on cache hit
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
        .context("failed to build request")?;

    let response = app
        .oneshot(req)
        .await
        .context("failed to process request")?;
    // Test: verify request without Authorization header returns 401 Unauthorized
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
        .context("failed to build request")?;

    let response = app
        .oneshot(req)
        .await
        .context("failed to process request")?;
    // Test: verify non-Bearer authorization scheme returns 401 Unauthorized
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
        .context("failed to build request")?;

    let response = app
        .oneshot(req)
        .await
        .context("failed to process request")?;
    // Test: verify forged token fails stateless MAC check without touching DB
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
    .await
    .context("failed to insert test user")?;

    let token = Session::create(&state.db, 1, &secret)
        .await
        .context("failed to create session token")?;

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
        .context("failed to build request")?;

    let response = app
        .oneshot(req)
        .await
        .context("failed to process request")?;
    // Test: verify valid token passes mac guard and injects TokenHash into extensions
    assert_eq!(response.status(), StatusCode::OK);

    Ok(())
}

#[test]
fn token_extractor_key_extraction() -> Result<()> {
    let extractor = TokenExtractor;
    let token_hash = TokenHash([5u8; 32]);

    // Request with TokenHash extension
    let mut req = Request::builder()
        .body(())
        .context("failed to build request")?;
    req.extensions_mut().insert(token_hash);

    let extracted = extractor.extract(&req);
    // Test: verify token hash is successfully extracted from request extensions
    assert!(extracted.is_ok());
    let extracted_key = extracted.context("extraction should succeed")?;
    assert_eq!(extracted_key, token_hash);

    // Request without TokenHash extension
    let empty_req = Request::builder()
        .body(())
        .context("failed to build empty request")?;
    let failed = extractor.extract(&empty_req);
    // Test: verify extraction returns error when TokenHash extension is absent
    assert!(failed.is_err());

    Ok(())
}
