use std::net::SocketAddr;

use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::Response,
};

use crate::{
    api::{
        Error::{BadRequest, Internal, Unauthorized},
        Result,
        auth::hashcash::HashCash,
    },
    app,
    error::internal::Error::MissingConnectInfo,
};

/// Middleware that enforces the stateless Hashcash Proof-of-Work protocol.
///
/// This guard runs on `POST /api/auth/login` and `POST /api/auth/register`.
/// It extracts the `X-Hashcash` header, delegates full cryptographic validation
/// to [`HashCash::verify_stateless`], and — on success — inserts the resulting
/// [`HashCash`] struct into the request extensions for the route handler to consume.
///
/// Placing this guard before the Argon2id-based handlers ensures that a client
/// must spend meaningful CPU time solving a SHA-256 puzzle before the server
/// commits any resources. This makes large-scale credential-stuffing attacks
/// economically prohibitive.
///
/// The `/api/auth/challenge` endpoint is deliberately excluded from this
/// middleware via the sequential `.route_layer()` order in [`route_main`].
///
/// [`route_main`]: crate::api::route::route_main
///
/// # Errors
///
/// - `400 Bad Request` — The `X-Hashcash` header is absent, contains non-UTF-8
///   bytes, exceeds the maximum allowed length of 168 bytes, or its length does
///   not match a valid `login` (165) or `register` (168) header.
/// - `401 Unauthorized` — The Proof-of-Work difficulty is insufficient, the
///   challenge has expired (older than 15 seconds), the timestamp is in the
///   future, or the BLAKE3 MAC does not match (indicating a forged or IP-stolen
///   challenge).
/// - `500 Internal Server Error` — `ConnectInfo` is absent from the request
///   extensions, which indicates the Axum server was started without
///   `into_make_service_with_connect_info`.
pub async fn pow_guard(
    State(state): State<app::State>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let header = request
        .headers()
        .get("X-Hashcash")
        .ok_or_else(|| Unauthorized("Missing X-Hashcash header".into()))?
        .to_str()
        .map_err(|_| BadRequest("Invalid X-Hashcash header encoding".into()))?;

    // Reject oversized headers immediately before any further processing.
    // The maximum valid header length is 168 bytes (for "register").
    if header.len() > 168 {
        return Err(BadRequest("X-Hashcash header too large".into()));
    }

    let ConnectInfo(addr) = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .ok_or(Internal(MissingConnectInfo))?;

    let hashcash =
        HashCash::verify_stateless(header, addr.ip(), &state.pow_secret)?;

    request.extensions_mut().insert(hashcash);

    Ok(next.run(request).await)
}
