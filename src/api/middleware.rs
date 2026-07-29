use crate::{
    api::Error::{BadRequest, Unauthorized},
    app::State as AppState,
    model::session::{Session, TokenHash},
};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use blake3::Hasher;

use crate::Result;

pub async fn auth_guard(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let header = request
        .headers()
        .get("Authorization")
        .ok_or(Unauthorized("Missing Header".into()))?;

    let token = header
        .to_str()
        .map_err(|_| BadRequest("Auth failed to serialize".into()))?
        .strip_prefix("Bearer ")
        .ok_or(Unauthorized(
            "Tokens must start with 'Bearer'".into(),
        ))?;

    let token_hash: TokenHash = Hasher::new()
        .update(token.as_bytes())
        .finalize()
        .as_bytes()
        .into();

    if let Some(user_id) = state.session_cache.get(&token_hash).await {
        request.extensions_mut().insert(user_id);
        return Ok(next.run(request).await);
    }

    let session = Session::verify(&state.db, token_hash.as_ref())
        .await?
        .ok_or(Unauthorized("Invalid or expired session".into()))?;

    state
        .session_cache
        .insert(token_hash, session.user_id)
        .await;

    request.extensions_mut().insert(session.user_id);

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use crate::app::AppStateBuilder;
    use axum::{Router, body::Body, http::Request, routing::get};
    use blake3::hash;
    use sqlx::sqlite::SqlitePoolOptions;
    use tower::ServiceExt;

    use super::*;

    // This test checks, that in-memory cache is being used first, instead of
    // querrying the database first.
    #[tokio::test]
    async fn auth_guard_bypasses_db_on_cache_hit() {
        // Even though we simulate establishing a connection to db,
        // but accessing this db will itself result in error.
        // and Ofcourse, this error will be bypassed if AppState::session_cache
        // returns the user_id, which is exactly what we want to know.
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();

        let mut state =
            AppStateBuilder::new().vault_path("/tmp").db(pool).build();

        let token = "top_secret";
        let token_hash =
            TokenHash::from(hash(token.as_bytes()).as_bytes());

        state.session_cache.insert(token_hash, 4).await;

        let app = Router::new()
            .route("/", get(|| async { "Success!" }))
            .route_layer(axum::middleware::from_fn_with_state(
                state.clone(),
                auth_guard,
            ))
            .with_state(state);

        let req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();

        // Test: We bypassed db querry for token check?
        assert_eq!(response.status(), 200);
    }
}
