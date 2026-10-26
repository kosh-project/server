use axum::{
    Extension, Json,
    extract::{Path, State},
};
use hyper::StatusCode;
use serde::Deserialize;

use crate::{
    Result,
    api::{self, Error::BadRequest},
    app,
};

#[derive(Debug, Deserialize)]
pub struct Request {
    pub payload: String,
}

impl Request {
    fn parse_payload(&self) -> api::Result<Vec<u8>> {
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
