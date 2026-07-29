use crate::api::Error::BadRequest;
use crate::app::State as AppState;
use crate::model::user::User;
use crate::{Error as AppErr, Result};
use axum::{Json, extract::State};
use hyper::StatusCode;
use serde::Deserialize;
use sqlx::Error as SqlErr;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub identity_hash: String,
    pub auth_verifier: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(register_request): Json<RegisterRequest>,
) -> Result<StatusCode> {
    let Ok(identity_hash) =
        hex::decode(&register_request.identity_hash)
    else {
        Err(BadRequest("identity_hash failed to decode".into()))?
    };

    let result = User::create(
        &state.db,
        identity_hash,
        register_request.auth_verifier,
    )
    .await;

    match result {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(AppErr::DatabaseError(SqlErr::Database(err)))
            if err.is_unique_violation() =>
        {
            Err(AppErr::Conflict("User already exists".into()))
        }
        Err(e) => Err(e.into()),
    }
}
