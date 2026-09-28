use std::{
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    api::{Error, Result},
    app,
    error::internal,
};
use axum::{
    Json,
    extract::{ConnectInfo, Query, State},
};
use blake3::Hasher;
use serde::{Deserialize, Serialize};

/// Query parameters accepted by `GET /api/auth/challenge`.
#[derive(Deserialize)]
pub struct ChallengeQuery {
    /// The endpoint the client intends to use the challenge for.
    ///
    /// Accepted values are `"login"` and `"register"`. The action is bound
    /// into the challenge MAC so that a challenge obtained for one action
    /// cannot be replayed against the other.
    pub action: String,
}

/// The JSON body returned by `GET /api/auth/challenge`.
#[derive(Serialize)]
pub struct Response {
    /// An opaque, stateless Proof-of-Work challenge string.
    ///
    /// The client must find a 16-byte nonce such that
    /// `SHA-256(identity_hash || nonce || challenge)` has at least 16 leading
    /// zero bits, then submit the full header as `X-Hashcash`.
    ///
    /// The challenge encodes a timestamp and is bound to the client's IP address
    /// via a BLAKE3 MAC. It expires after 15 seconds.
    pub challenge: String,
}

/// `GET /api/auth/challenge?action=<login|register>`
///
/// Generates a stateless, ephemeral Proof-of-Work challenge bound to the
/// client's IP address and the intended action. The server stores zero state.
///
/// The challenge string has the following fixed-width layout:
///
/// ```text
/// [ timestamp_hex (16) ][ mac_hex (64) ][ action (5 or 8) ]
/// ```
///
/// The client embeds this challenge in the `X-Hashcash` header alongside an
/// `identity_hash` and a nonce that satisfies the SHA-256 difficulty target.
///
/// # Errors
/// - Returns `400 Bad Request` if `action` is not `"login"` or `"register"`.
/// - Returns a `500 Internal Server Error` if the system clock is set before
///   the Unix epoch.
pub async fn generate(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<app::State>,
    Query(query): Query<ChallengeQuery>,
) -> Result<Json<Response>> {
    if query.action != "login" && query.action != "register" {
        return Err(Error::BadRequest("Invalid action".into()));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(internal::Error::from)?
        .as_secs();

    let timestamp_hex = format!("{now:016x}");

    // Stream timestamp, raw IP bytes, and action into the BLAKE3 hasher.
    // Using raw octets instead of formatting the IP as a string avoids a
    // heap allocation and makes the byte sequence unambiguous across
    // IPv4-mapped IPv6 addresses.
    let mut hasher = Hasher::new_keyed(&state.pow_secret.key());
    hasher.update(timestamp_hex.as_bytes());
    match addr.ip() {
        IpAddr::V4(ip) => hasher.update(&ip.octets()),
        IpAddr::V6(ip) => hasher.update(&ip.octets()),
    };
    hasher.update(query.action.as_bytes());
    let mac = hasher.finalize();

    // Layout: timestamp(16) + mac(64) + action(5 or 8).
    // The MAC precedes the variable-length action so that all fixed-width
    // fields in the corresponding X-Hashcash header sit at deterministic offsets.
    let challenge = format!("{timestamp_hex}{}{}", mac.to_hex(), query.action);

    Ok(Json(Response { challenge }))
}
