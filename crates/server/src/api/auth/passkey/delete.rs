use axum::{
    Extension,
    extract::{Path, State},
};
use hyper::StatusCode;

use crate::{
    Result,
    api::Error::{BadRequest, NotFound},
    app,
    model::passkey,
};

pub async fn delete(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Path(credential_id): Path<String>,
) -> Result<StatusCode> {
    let credential_id = hex::decode(credential_id)
        .map_err(|_| BadRequest("credential_id must be valid hex"))?;

    let deleted =
        passkey::Record::delete(&state.db, &credential_id, user_id).await?;

    if !deleted {
        Err(NotFound("Passkey credential not found"))?;
    }

    Ok(StatusCode::NO_CONTENT)
}
