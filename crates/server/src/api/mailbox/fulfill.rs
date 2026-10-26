//! Route handler for fulfilling an ephemeral ECDH key exchange mailbox.

use axum::{
    Extension, Json,
    extract::{Path, State},
};
use hyper::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{
    Result,
    api::{self, Error::BadRequest},
    app,
};

/// Request payload for `POST /api/v1/devices/mailbox/{id}/fulfill`.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Request {
    /// Hexadecimal-encoded ECDH key exchange payload (between 120 and 8192 hex characters).
    pub payload: String,
}

impl Request {
    /// Validates length bounds and decodes the hexadecimal payload into raw bytes.
    ///
    /// Enforces that `payload` is between 120 and 8,192 hexadecimal characters (inclusive),
    /// corresponding to 60 to 4,096 raw bytes. Both uppercase and lowercase ASCII hex
    /// representations are accepted.
    ///
    /// # Errors
    ///
    /// - Returns [`BadRequest`] if the payload length is outside [120, 8192] hex characters.
    /// - Returns [`BadRequest`] if the payload contains invalid hexadecimal characters.
    pub(crate) fn parse_payload(&self) -> api::Result<Vec<u8>> {
        if self.payload.len() < 120 || self.payload.len() > 8_192 {
            return Err(BadRequest(
                "Payload must be between 120 and 8192 hex chars (or 60 to 4096 bytes)",
            ));
        }

        let Ok(bytes) = hex::decode(&self.payload) else {
            return Err(BadRequest("Invalid hexadecimal payload"));
        };

        Ok(bytes)
    }
}

/// Fulfills an ephemeral mailbox by depositing a client payload.
///
/// Route: `POST /api/v1/devices/mailbox/{id}/fulfill` (protected by [`mac_guard`] and [`auth_guard`]).
///
/// 1. Validates that `payload` is between 120 and 8192 hexadecimal characters (60 to 4096 raw bytes).
/// 2. Hex-decodes the payload into raw bytes.
/// 3. Deposits the payload into the mailbox atomically if it is currently `Pending`.
///
/// Returns HTTP [`StatusCode::NO_CONTENT`] on successful fulfillment.
///
/// # Errors
///
/// - Returns [`BadRequest`] if the payload bounds check fails or hex decoding fails.
/// - Returns [`NotFound`](crate::api::Error::NotFound) if the mailbox does not exist, has expired, belongs to another user, or has already been consumed.
/// - Returns [`BadRequest`] if the mailbox has already been fulfilled.
/// - Returns [`crate::error::internal::Error`] if the internal mailbox lock is poisoned.
///
/// [`auth_guard`]: crate::api::middleware::auth_guard
/// [`mac_guard`]: crate::api::middleware::mac_guard
pub async fn fulfill(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Path(mailbox_id): Path<String>,
    Json(request): Json<Request>,
) -> Result<StatusCode> {
    let raw_payload = request.parse_payload()?;

    state
        .mailbox
        .fulfill(&mailbox_id, user_id, raw_payload)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
