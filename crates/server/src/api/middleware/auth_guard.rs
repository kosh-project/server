use crate::{
    api::Error::Unauthorized,
    app::State as AppState,
    model::session::{Session, TokenHash},
};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::Result;

/// Middleware that guards protected routes by validating the session token.
///
/// Extracts the Bearer token from the `Authorization` header, computes its hash,
/// and checks the in-memory cache. If missing from the cache, it verifies the
/// token against the database and caches the result for future requests.
///
/// # Errors
/// - Returns an `Unauthorized` if the `Authorization` header is missing, malformed, or contains an invalid/expired token.
/// - Returns a `BadRequest` if the token string cannot be serialized.
/// - Returns an internal error if a database query fails.
pub async fn auth_guard(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let token_hash = request
        .extensions()
        .get::<TokenHash>()
        .ok_or(Unauthorized("Unverified token"))?;

    if let Some(user_id) = state.session_cache.get(token_hash).await {
        request.extensions_mut().insert(user_id);
        return Ok(next.run(request).await);
    }

    let session = Session::verify(&state.db, token_hash.as_ref())
        .await?
        .ok_or(Unauthorized("Invalid or expired session"))?;

    state
        .session_cache
        .insert(token_hash.to_owned(), session.user_id)
        .await;

    request.extensions_mut().insert(session.user_id);

    Ok(next.run(request).await)
}
