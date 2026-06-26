use crate::api::Error as ApiError;
use crate::app::State as AppState;
use crate::{Error as AppErr, Result};
use axum::{Json, extract::State};
use hyper::StatusCode;
use serde::Deserialize;
use sqlx::{Error as SqlErr, Executor};

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub identity_hash: String,
    pub auth_verifier: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(register_request): Json<RegisterRequest>,
) -> Result<StatusCode> {
    let hash_bytes = hex::decode(register_request.identity_hash)
        .map_err(|_| ApiError::BadRequest("Invalid hex string".into()))?;

    let result = sqlx::query!(
        "INSERT INTO users (identity_hash, auth_verifier) VALUES (?, ?)",
        hash_bytes,
        register_request.auth_verifier
    )
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => Ok(StatusCode::CREATED),
        Err(SqlErr::Database(db_err)) if db_err.is_unique_violation() => {
            Err(AppErr::Conflict("User already exists".into()))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn login() {}
