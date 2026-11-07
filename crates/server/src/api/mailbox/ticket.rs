//! Route handler for minting a device-pairing registration ticket.

use axum::{Extension, Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{
    app::{self, Ticket},
    error::JsonResult,
};

const fn default_ttl() -> u64 {
    Ticket::TTL_SECS
}

/// Response payload returned by `POST /api/v1/devices/link-ticket`.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct Response {
    /// 64-character hexadecimal single-use registration ticket.
    pub ticket: String,
    /// Protocol alias for blueprint specification compatibility.
    pub registration_ticket: String,
    /// Time-to-live in seconds before the ticket expires (300 seconds).
    #[serde(default = "default_ttl")]
    pub expires_in: u64,
}

impl Response {
    /// Constructs a response envelope containing the minted ticket.
    #[must_use]
    pub fn new(ticket: String) -> Self {
        Self {
            registration_ticket: ticket.clone(),
            ticket,
            expires_in: Ticket::TTL_SECS,
        }
    }
}

/// Mints a short-lived single-use registration ticket for device pairing.
///
/// Route: `POST /api/v1/devices/link-ticket` (protected by [`mac_guard`] and [`auth_guard`]).
///
/// Generates a 32-byte (64 hex char) cryptographically secure registration ticket bound to
/// the caller's `user_id` with a 300-second Time-To-Live. The authenticated device includes
/// this ticket in the ECDH payload sent to the pairing device.
///
/// # Errors
///
/// This handler is infallible under normal operation; it always returns `Ok(Json(Response))`.
///
/// [`auth_guard`]: crate::api::middleware::auth_guard
/// [`mac_guard`]: crate::api::middleware::mac_guard
pub async fn ticket(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
) -> JsonResult<Response> {
    let token = state.ticket.mint(user_id).await;

    Ok(Json(Response::new(token)))
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::*;

    #[test]
    fn ticket_response_serialization_and_roundtrip() {
        let resp = Response::new(
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                .to_string(),
        );
        let expected_json = json!({
            "ticket": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "registration_ticket": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "expires_in": 300
        });

        assert_eq!(
            serde_json::to_value(&resp).ok(),
            Some(expected_json.clone())
        );
        assert_eq!(
            serde_json::from_value::<Response>(expected_json).ok(),
            Some(resp)
        );
    }
}
