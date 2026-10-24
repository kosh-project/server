use crate::Result;
use crate::api::Error::BadRequest;
use crate::api::auth::hashcash::HashCash;
use crate::app::State as AppState;
use crate::model::user::User;
use axum::Extension;
use axum::{Json, extract::State};
use hyper::StatusCode;
use serde::Deserialize;

/// The JSON body expected by the registration endpoint.
#[derive(Deserialize)]
pub struct Request {
    /// The authentication verifier derived from the user's credentials on the client side.
    ///
    /// The server stores this string verbatim and compares it on login. It is the
    /// client's responsibility to derive a strong verifier (e.g., using Argon2id)
    /// before sending it — the server does not perform any additional hashing.
    pub auth_verifier: String,

    /// 60-byte ChaCha20-Poly1305 wrapped root vault key encoded as 120 hex characters.
    pub wrapped_vault_key: String,
}

/// Registers a new user with the provided credentials.
///
/// # Errors
/// - Returns a `BadRequest` if the identity hash cannot be decoded from hex.
/// - Returns a `BadRequest` if the `wrapped_vault_key` length is invalid or cannot be decoded from hex.
/// - Returns a `Conflict` if a user with the same identity hash already exists.
/// - Returns an internal error if a database query fails.
pub async fn register(
    State(state): State<AppState>,
    Extension(hashcash): Extension<HashCash>,
    Json(request): Json<Request>,
) -> Result<StatusCode> {
    let Ok(identity_hash) = hex::decode(&hashcash.identity_hash) else {
        Err(BadRequest("identity_hash failed to decode"))?
    };

    if request.wrapped_vault_key.len() != 120 {
        Err(BadRequest("wrapped_vault_key must be exactly 60 bytes"))?;
    }

    let Ok(wrapped_vault_key) = hex::decode(request.wrapped_vault_key) else {
        Err(BadRequest("wrapped_vault_key seems corrupted"))?
    };

    User::create(
        &state.db,
        &identity_hash,
        request.auth_verifier,
        &wrapped_vault_key,
    )
    .await?;

    Ok(StatusCode::CREATED)
}
