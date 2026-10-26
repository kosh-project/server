//! Route handler for polling and picking up payload from an ephemeral ECDH mailbox.

use axum::{
    Extension, Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

use crate::{app, error::JsonResult};

/// Response payload returned by `GET /api/v1/devices/mailbox/{id}`.
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

/// Polls status or retrieves the deposited payload from an ephemeral mailbox.
///
/// Route: `GET /api/v1/devices/mailbox/{id}` (protected by [`mac_guard`] and [`auth_guard`]).
///
/// 1. Verifies that the mailbox exists and belongs to the authenticated caller.
/// 2. If the mailbox is still in `Pending` state, returns `{"status": "pending"}` without eviction.
/// 3. If fulfilled, atomically transitions status to `Consumed`, evicts the mailbox from cache
///    (single-use "burn-after-reading" semantics), and returns `{"status": "fulfilled", "payload": "<hex>"}`.
///
/// # Errors
///
/// - Returns [`NotFound`](crate::api::Error::NotFound) if the mailbox does not exist, has expired,
///   belongs to a different user, or has already been picked up.
/// - Returns [`crate::error::internal::Error`] if the internal mailbox lock is poisoned.
///
/// [`auth_guard`]: crate::api::middleware::auth_guard
/// [`mac_guard`]: crate::api::middleware::mac_guard
pub async fn pickup(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Path(mailbox_id): Path<String>,
) -> JsonResult<Response> {
    if let Some(payload) = state.mailbox.pickup(&mailbox_id, user_id).await? {
        return Ok(Json(Fulfilled {
            payload: hex::encode(payload),
        }));
    }

    Ok(Json(Pending))
}
