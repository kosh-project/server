//! Route handler for polling and picking up payload from an ephemeral ECDH mailbox.

use axum::{
    Json,
    extract::{Path, State},
};
use hyper::HeaderMap;
use serde::{Deserialize, Serialize};

use crate::{api::Error::Unauthorized, app, error::JsonResult};

/// Response payload returned by `GET /api/auth/mailbox/{id}/pickup`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Response {
    /// Mailbox is awaiting fulfillment by the sender device.
    Pending,
    /// Mailbox was fulfilled; contains the hex-encoded payload.
    Fulfilled {
        /// Hexadecimal-encoded payload deposited by the sender device.
        payload: String,
    },
}

use Response::{Fulfilled, Pending};

/// Polls status or retrieves the deposited payload from an ephemeral mailbox (Flow A & B).
///
/// Route: `GET /api/auth/mailbox/{id}/pickup` (unauthenticated, rate-limited by `with_auth_ip_limit`).
///
/// 1. Extracts `polling_token` from the `Authorization: Bearer <token>` header.
/// 2. Verifies the token against the constant-time BLAKE3 hash stored in the mailbox entry.
/// 3. If the mailbox is still in `Ready` state, returns `{"status": "pending"}` without eviction.
/// 4. If fulfilled, atomically transitions status to `Consumed`, evicts the mailbox from cache
///    (single-use "burn-after-reading" semantics), and returns `{"status": "fulfilled", "payload": "<hex>"}`.
///
/// # Errors
///
/// - Returns [`Unauthorized`] if the `Authorization` header is missing or malformed.
/// - Returns [`NotFound`](crate::app::mailbox::Error::NotFound) if the mailbox does not exist, has expired,
///   the polling token is invalid, or the payload has already been picked up (anti-enumeration).
/// - Returns [`crate::error::internal::Error`] if the internal mailbox lock is poisoned.
pub async fn pickup(
    State(state): State<app::State>,
    Path(mailbox_id): Path<String>,
    headers: HeaderMap,
) -> JsonResult<Response> {
    let token = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(Unauthorized("Missing or invalid Authorization header"))?;

    if let Some(payload) = state.mailbox.pickup(&mailbox_id, token).await? {
        return Ok(Json(Fulfilled {
            payload: hex::encode(payload),
        }));
    }

    Ok(Json(Pending))
}
