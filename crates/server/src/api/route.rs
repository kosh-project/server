use axum::{
    Json, Router,
    extract::Request,
    middleware,
    routing::{delete, get, post, put},
};
use serde_json::{Value, json};

use crate::{
    api::{
        self,
        assets::{self, list},
        auth::{self, challenge, passkey, pow_guard},
        mailbox,
        middleware::{
            // auth_ip_limiter, device_limiter, global_ip_limiter,
            RateLimitExt,
            auth_guard,
            log_middleware,
            mac_guard,
        },
        sync,
    },
    app::State as AppState,
    logger::logging_enabled,
};

/// Constructs the top-level Axum router for the server.
///
/// Composes all sub-routers and, if the logging service is active, wraps
/// the entire stack in the telemetry middleware that records error events
/// to the background logger.
///
/// This function should be called exactly once during server startup. The
/// resulting `Router` is passed directly to `axum::serve`.
pub fn route_main(state: AppState) -> Router {
    let routes = Router::new()
        .route("/health", get(health))
        .nest("/api/auth", auth_route(&state))
        .nest("/api/v1", protected_routes(&state))
        .with_global_ip_limit()
        .with_state(state);

    if logging_enabled() {
        return routes.layer(middleware::from_fn(log_middleware));
    }

    routes
}

/// Constructs the unauthenticated authentication sub-router.
///
/// These endpoints do not require a Bearer token and are intentionally
/// excluded from the `auth_guard` middleware layer. Rate limiting is applied
/// per IP address via `auth_ip_limit`.
///
/// | Method | Path | Handler | Description |
/// |--------|------|---------|-------------|
/// | `POST` | `/api/auth/register` | [`api::auth::register`] | Password-based user registration with `PoW` |
/// | `POST` | `/api/auth/login` | [`api::auth::login`] | Password-based user login with `PoW` |
/// | `GET`  | `/api/auth/challenge` | [`challenge::generate`] | Issues stateless Hashcash `PoW` challenge |
/// | `POST` | `/api/auth/passkey/login/start` | [`passkey::login_start`] | Initiates discoverable passkey login ceremony |
/// | `POST` | `/api/auth/passkey/login/finish` | [`passkey::login_finish`] | Completes passkey login and issues session token |
fn auth_route(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/register", post(api::auth::register))
        .route("/login", post(api::auth::login))
        .route_layer(middleware::from_fn_with_state(state.clone(), pow_guard))
        .route("/challenge", get(challenge::generate))
        .route("/passkey/login/start", post(passkey::login_start))
        .route("/passkey/login/finish", post(passkey::login_finish))
        .with_auth_ip_limit()
}

/// Constructs the authenticated sub-router for all protected endpoints.
///
/// Every route registered here is wrapped by [`auth_guard`], which validates
/// the `Authorization: Bearer <token>` header and injects the resolved
/// `user_id` into the request extensions before the handler is called. In addition,
/// requests pass through [`mac_guard`] to verify session token cryptographic signatures
/// and [`RateLimitExt::with_device_limit`] to throttle abusive callers.
///
/// | Method | Path | Handler | Description |
/// |--------|------|---------|-------------|
/// | `POST` | `/api/v1/upload/{tag}` | [`assets::upload`] | Uploads a content-addressed binary asset blob |
/// | `GET` | `/api/v1/assets` | [`list`] | Lists assets with cursor pagination and tag filters |
/// | `GET` | `/api/v1/assets/{hash}` | [`assets::get`] | Streams an asset by `BLAKE3` hash |
/// | `DELETE` | `/api/v1/assets/{hash}` | [`assets::delete`] | Decrements ownership reference count for an asset |
/// | `GET` | `/api/v1/storage` | [`storage`] | Checks storage subsystem availability |
/// | `POST` | `/api/v1/sync/delta` | [`api::sync::append_delta`] | Appends delta sync segment |
/// | `GET` | `/api/v1/sync/delta` | [`api::sync::stream_delta`] | Streams delta sync segments |
/// | `DELETE` | `/api/v1/sync/prune` | [`api::sync::prune_ledger`] | Prunes merged delta sync segments |
/// | `GET` | `/api/v1/sync/snapshot` | [`api::sync::latest_snapshot`] | Retrieves the latest state snapshot |
/// | `POST` | `/api/v1/sync/snapshot` | [`api::sync::register_snapshot`] | Registers a new state snapshot |
/// | `PUT` | `/api/v1/auth/recovery` | [`auth::rotate`] | Rotates user authentication and recovery credentials |
/// | `POST` | `/api/v1/auth/passkey/register/start` | [`passkey::register_start`] | Generates passkey creation challenge |
/// | `POST` | `/api/v1/auth/passkey/register/finish` | [`passkey::register_finish`] | Completes passkey registration and persists credential |
/// | `GET` | `/api/v1/auth/passkey` | [`passkey::list`] | Lists all registered passkeys for caller |
/// | `DELETE` | `/api/v1/auth/passkey/{credential_id}` | [`passkey::delete`] | Deletes a registered passkey owned by caller |
/// | `POST` | `/api/v1/devices/mailbox` | [`mailbox::create`] | Allocates ephemeral ECDH mailbox |
/// | `POST` | `/api/v1/devices/mailbox/{id}/fulfill` | [`mailbox::fulfill`] | Deposits payload into mailbox |
/// | `GET` | `/api/v1/devices/mailbox/{id}` | [`mailbox::pickup`] | Polls or picks up mailbox payload |
fn protected_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/upload/{tag}", post(assets::upload))
        .route("/assets", get(list))
        .route("/assets/{hash}", get(assets::get).delete(assets::delete))
        .route("/storage", get(storage))
        .route("/sync/delta", post(sync::append_delta))
        .route("/sync/delta", get(sync::stream_delta))
        .route("/sync/prune", delete(sync::prune_ledger))
        .route("/sync/snapshot", get(sync::latest_snapshot))
        .route("/sync/snapshot", post(sync::register_snapshot))
        .route("/auth/recovery", put(auth::rotate))
        .route(
            "/auth/passkey/register/start",
            post(passkey::register_start),
        )
        .route(
            "/auth/passkey/register/finish",
            post(passkey::register_finish),
        )
        .route("/auth/passkey/{credential_id}", delete(passkey::delete))
        .route("/auth/passkey", get(passkey::list))
        // Ephemeral blind ECDH mailbox relay for device pairing
        .route("/devices/mailbox", post(mailbox::create))
        .route("/devices/mailbox/{id}/fulfill", post(mailbox::fulfill))
        .route("/devices/mailbox/{id}", get(mailbox::pickup))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_guard))
        .with_device_limit()
        .route_layer(middleware::from_fn_with_state(state.clone(), mac_guard))
}

/// `GET /health`
///
/// Returns `{ "health": "ok" }` to confirm the server process is running.
/// This endpoint is unauthenticated and does not hit the database or filesystem.
/// It is suitable for use as a liveness probe in container orchestration.
async fn health() -> Json<Value> {
    Json(json!({
        "health" : "ok"
    }))
}

/// `GET /api/v1/storage`
///
/// Returns basic information about the server's storage configuration.
/// Currently returns a static JSON response confirming the storage root exists.
/// Protected by `auth_guard`.
async fn storage(_: Request) -> Json<Value> {
    Json(json!({
        "root" : "/storage",
        "exists" : true,
    }))
}
