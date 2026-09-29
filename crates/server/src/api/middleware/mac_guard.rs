use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use blake3::Hasher;

use crate::{
    api::{
        Error::{BadRequest, Unauthorized},
        Result,
    },
    app,
    model::session::{Session, TokenHash},
};

/// Middleware that performs a stateless BLAKE3 pre-filter on every Bearer token.
///
/// This is Layer 2 of the authenticated ingress funnel, sitting immediately after
/// the global IP rate limiter and before the session cache / database lookup.
///
/// It extracts the `Authorization: Bearer <token>` header, verifies the token's
/// BLAKE3 MAC and expiry timestamp against `K_server` without touching the
/// database, then computes a [`TokenHash`] and injects it into the request
/// extensions. Downstream middleware (the device governor and `auth_guard`) use
/// this hash as an opaque, stack-allocated cache key.
///
/// Because forged or sprayed tokens are rejected here in under 1 microsecond,
/// the session cache and SQLite database are never queried for tokens that were
/// never issued by this server. This prevents cache-miss floods from turning into
/// random disk seeks on the mechanical storage.
///
/// # Errors
///
/// - `401 Unauthorized` — The `Authorization` header is missing, the value does
///   not begin with `"Bearer "`, the token fails the BLAKE3 MAC check, or the
///   token's embedded expiry timestamp has passed.
/// - `400 Bad Request` — The header value contains bytes that are not valid UTF-8.
pub async fn mac_guard(
    State(state): State<app::State>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let header = request
        .headers()
        .get("Authorization")
        .ok_or(Unauthorized("Missing Authorization header"))?;

    let token = header
        .to_str()
        .map_err(|_| BadRequest("Authorization header is not valid UTF-8"))?
        .strip_prefix("Bearer ")
        .ok_or(Unauthorized(
            "Authorization header must start with 'Bearer '",
        ))?;

    Session::verify_stateless(token, &state.secret)?;

    let token_hash: TokenHash =
        Hasher::new().update(token.as_bytes()).finalize().into();

    request.extensions_mut().insert(token_hash);

    Ok(next.run(request).await)
}
