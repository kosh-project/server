use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{
    api::Error::BadRequest,
    app::{self, Mailbox},
    error::JsonResult,
};

#[derive(Debug, Deserialize)]
pub struct Request {
    pub pub_temp: String,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub mailbox_id: String,
    pub polling_token: String,
    pub expires_in: u64,
}

pub async fn open(
    State(state): State<app::State>,
    Json(request): Json<Request>,
) -> JsonResult<Response> {
    if request.pub_temp.len() != 64 {
        Err(BadRequest(
            "pub_temp must be exactly 64 hex characters (32 bytes)",
        ))?;
    }

    let pub_temp = hex::decode(&request.pub_temp)
        .map_err(|_| BadRequest("Invalid hexadecimal in pub_temp"))?;

    let receipt = state.mailbox.open(pub_temp).await;

    Ok(Json(Response {
        mailbox_id: receipt.id,
        polling_token: receipt.token.hex,
        expires_in: Mailbox::TTL_SECS,
    }))
}
