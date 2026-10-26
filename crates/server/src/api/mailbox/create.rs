use axum::{Extension, Json, extract::State};
use serde::Serialize;

use crate::{
    app::{self, Mailbox},
    error::JsonResult,
};

#[derive(Debug, Serialize)]
pub struct Response {
    pub mailbox_id: String,
    pub expires_in: u64,
}

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
