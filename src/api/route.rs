use axum::{
    Json, Router,
    extract::Request,
    middleware,
    routing::{get, post},
};
use serde_json::{Value, json};

use crate::{
    api::{
        self,
        assets,
        middleware::auth_guard,
    },
    app::State as AppState,
    log,
};

/// ## Main router
/// Rouutes `/` and all the underlying routes nested
/// within the server. \
/// To be used when starting server.
pub fn route_main(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .nest("/api/auth", auth_route())
        .nest("/api/v1", protected_routes(&state))
        .with_state(state)
}

/// ## Authentication endpoints
fn auth_route() -> Router<AppState> {
    Router::new()
        .route("/register", post(api::auth::register))
        .route("/login", post(api::auth::login))
}

/// ## Protected Routes
/// Endpoints that require client to hold a token, for
/// their requests to be handled. \
/// All the endpoints listed here, fall behind the
/// [`crate::api::middleware::auth_guard`]
fn protected_routes(state: &AppState) -> Router<AppState> {
    Router::new()
        .route("/upload/{tag}", post(assets::upload))
        .route(
            "/assets/{hash}",
            get(assets::get).delete(assets::delete),
        )
        .route("/storage", get(storage))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_guard,
        ))
}

async fn health() -> Json<Value> {
    log!("HANDLER", "get_health");
    Json(json!({
        "health" : "ok"
    }))
}

async fn storage(_: Request) -> Json<Value> {
    Json(json!({
        "root" : "/storage",
        "exists" : true,
    }))
}
