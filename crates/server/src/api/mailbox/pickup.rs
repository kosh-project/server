use axum::{
    Extension, Json,
    extract::{Path, State},
};
use serde::Serialize;

use crate::{app, error::JsonResult};

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Response {
    Pending,
    Fulfilled { payload: String },
}

use Response::{Fulfilled, Pending};

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
