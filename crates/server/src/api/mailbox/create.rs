//! Route handler for allocating an ephemeral ECDH key exchange mailbox.

use axum::{Extension, Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{
    app::{self, Mailbox},
    error::JsonResult,
};

/// Response payload returned by `POST /api/v1/devices/mailbox`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Response {
    /// 8-character lowercase Crockford Base32 mailbox identifier.
    pub mailbox_id: String,
    /// Time-to-live in seconds before the mailbox expires (configured by [`Mailbox::TTL_SECS`]).
    pub expires_in: u64,
}

/// Creates a new ephemeral mailbox for out-of-band ECDH key exchange.
///
/// Route: `POST /api/v1/devices/mailbox` (protected by [`mac_guard`] and [`auth_guard`]).
///
/// Allocates a unique 8-character Crockford Base32 mailbox identifier bound to the caller's
/// `user_id` with an initial `Pending` status and a 300-second Time-To-Live ([`Mailbox::TTL_SECS`]).
///
/// # Errors
///
/// This handler is infallible under normal operation; it always returns `Ok(Json(Response))`.
///
/// [`auth_guard`]: crate::api::middleware::auth_guard
/// [`mac_guard`]: crate::api::middleware::mac_guard
pub async fn create(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
) -> JsonResult<Response> {
    let mailbox_id = state.mailbox.create(user_id).await;

    Ok(Json(Response {
        mailbox_id,
        expires_in: Mailbox::TTL_SECS,
    }))
}
