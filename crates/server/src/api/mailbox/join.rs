use axum::{
    Json,
    extract::{Path, State},
};
use serde::{Deserialize, Serialize};

use crate::{api::Error::BadRequest, app, error::JsonResult};

#[derive(Debug, Deserialize)]
pub struct Request {
    pub pub_temp: String,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub token: String,
}

pub async fn join(
    State(state): State<app::State>,
    Path(mailbox_id): Path<String>,
    Json(request): Json<Request>,
) -> JsonResult<Response> {
    if request.pub_temp.len() != 64 {
        Err(BadRequest(
            "pub_temp must be exactly 64 hex characters (32 bytes)",
        ))?;
    }

    let pub_temp = hex::decode(&request.pub_temp)
        .map_err(|_| BadRequest("Invalid hexadecimal in pub_temp"))?;

    let token = state.mailbox.join(&mailbox_id, pub_temp).await?;

    Ok(Json(Response { token }))
}
