use axum::{
    Extension, Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{app, error::JsonResult};

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Response {
    Waiting,
    Ready { pub_temp: String },
}

/// Polls the status of an offered pairing mailbox (Flow B).
///
/// Route: `GET /api/v1/devices/mailbox/{id}/status` (protected by `mac_guard` and `auth_guard`).
///
/// # Errors
///
/// - Returns [`NotFound`](crate::app::mailbox::Error::NotFound) if the mailbox does not exist or caller is unauthorized.
/// - Returns [`AlreadyFulfilled`](crate::app::mailbox::Error::AlreadyFulfilled) if the mailbox has already been fulfilled.
pub async fn status(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Path(mailbox_id): Path<String>,
) -> JsonResult<Response> {
    if let Some(pub_temp) = state.mailbox.status(&mailbox_id, user_id).await? {
        return Ok(Json(Response::Ready {
            pub_temp: hex::encode(pub_temp),
        }));
    }

    Ok(Json(Response::Waiting))
}
