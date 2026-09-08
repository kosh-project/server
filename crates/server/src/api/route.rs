use axum::{
    Json, Router,
    extract::Request,
    middleware,
    routing::{delete, get, post},
};
use serde_json::{Value, json};

use crate::{
    api::{
        self,
        assets::{self, list},
        middleware::{auth_guard, log_middleware},
    },
    app::State as AppState,
    info,
    logger::{Module, logging_enabled},
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
        .nest("/api/auth", auth_route())
        .nest("/api/v1", protected_routes(&state))
        .with_state(state);

    if logging_enabled() {
        return routes.layer(middleware::from_fn(log_middleware));
    }

    routes
}

/// Constructs the unauthenticated authentication sub-router.
///
/// These endpoints do not require a Bearer token and are intentionally
/// excluded from the `auth_guard` middleware layer.
///
/// | Method | Path | Handler |
/// |--------|------|---------|
/// | `POST` | `/api/auth/register` | [`api::auth::register`] |
/// | `POST` | `/api/auth/login` | [`api::auth::login`] |
fn auth_route() -> Router<AppState> {
    Router::new()
        .route("/register", post(api::auth::register))
        .route("/login", post(api::auth::login))
}

/// Constructs the authenticated sub-router for all protected endpoints.
///
/// Every route registered here is wrapped by [`auth_guard`], which validates
/// the `Authorization: Bearer <token>` header and injects the resolved
/// `user_id` into the request extensions before the handler is called.
///
/// | Method | Path | Handler |
/// |--------|------|---------|
/// | `POST` | `/api/v1/upload/{tag}` | [`assets::upload`] |
/// | `GET` | `/api/v1/assets/{hash}` | [`assets::get`] |
/// | `DELETE` | `/api/v1/assets/{hash}` | [`assets::delete`] |
/// | `GET` | `/api/v1/storage` | [`storage`] |
/// | `POST` | `/api/v1/sync/delta` | [`api::sync::append_delta`] |
/// | `GET` | `/api/v1/sync/delta` | [`api::sync::stream_delta`] |
/// | `DELETE` | `/api/v1/sync/prune` | [`api::sync::prune_ledger`] |
fn protected_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/upload/{tag}", post(assets::upload))
        .route("/assets", get(list))
        .route("/assets/{hash}", get(assets::get).delete(assets::delete))
        .route("/storage", get(storage))
        .route("/sync/delta", post(api::sync::append_delta))
        .route("/sync/delta", get(api::sync::stream_delta))
        .route("/sync/prune", delete(api::sync::prune_ledger))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_guard))
}

/// `GET /health`
///
/// Returns `{ "health": "ok" }` to confirm the server process is running.
/// This endpoint is unauthenticated and does not hit the database or filesystem.
/// It is suitable for use as a liveness probe in container orchestration.
async fn health() -> Json<Value> {
    info!(Module::Api, "get_health");
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
