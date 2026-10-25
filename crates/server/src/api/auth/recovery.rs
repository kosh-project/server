use crate::{Result, api::Error::Unauthorized};
use axum::{Extension, Json, extract::State};
use hyper::StatusCode;
use serde::Deserialize;

use crate::{
    api::Error::BadRequest,
    app,
    model::{
        session::{Session, TokenHash},
        user::User,
    },
};

/// The JSON request body expected by the recovery credential rotation endpoint.
#[derive(Deserialize)]
pub struct Request {
    /// 32-byte BLAKE3 identity hash encoded as 64 hex characters.
    pub identity_hash: String,
    /// Argon2id authentication verifier string.
    pub auth_verifier: String,
    /// 60-byte ChaCha20-Poly1305 wrapped vault key encoded as 120 hex characters.
    pub wrapped_vault_key: String,
    /// Whether to terminate all other active sessions for this user.
    #[serde(default)]
    pub revoke_other_sessions: bool,
}

pub struct Validated {
    pub identity_hash: [u8; 32],
    pub wrapped_vault_key: Vec<u8>,
}

impl Request {
    /// Decodes and validates the identity hash and wrapped vault key.
    ///
    /// # Errors
    /// Returns `BadRequest` if lengths or hex decoding fails.
    pub fn decode(&self) -> Result<Validated> {
        if self.identity_hash.len() != 64 {
            Err(BadRequest(
                "identity_hash must be exactly 64 hex characters",
            ))?;
        }
        let identity_hash_vec = hex::decode(&self.identity_hash)
            .map_err(|_| BadRequest("identity_hash failed to hex-decode"))?;

        let identity_hash: [u8; 32] = identity_hash_vec
            .try_into()
            .map_err(|_| BadRequest("invalid identity_hash length"))?;

        if self.wrapped_vault_key.len() != 120 {
            Err(BadRequest(
                "wrapped_vault_key must be exactly 120 hex characters",
            ))?;
        }
        let wrapped_vault_key =
            hex::decode(&self.wrapped_vault_key).map_err(|_| {
                BadRequest("wrapped_vault_key failed to hex-decode")
            })?;

        Ok(Validated {
            identity_hash,
            wrapped_vault_key,
        })
    }
}

/// Rotates the authenticated user's recovery phrase, auth verifier, and wrapped root vault key.
///
/// Executes atomically inside an SQLite transaction. If `revoke_other_sessions` is true,
/// all other active bearer tokens belonging to this user (except the caller's current session)
/// are deleted from SQLite and invalidated in the in-memory session cache.
///
/// # Errors
/// - Returns [`BadRequest`] if hex decoding or field length validation fails.
/// - Returns [`Unauthorized`] if the user does not exist.
/// - Returns [`crate::model::Error::UniqueViolation`] if the new `identity_hash` is already registered.
/// - Returns internal error if database transaction or query fails.
pub async fn rotate(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Extension(token_hash): Extension<TokenHash>,
    Json(request): Json<Request>,
) -> Result<StatusCode> {
    let validated = request.decode()?;

    let mut txn = state.db.begin().await?;

    let rows_affected = User::rotate_recovery(
        &mut *txn,
        user_id,
        &validated.identity_hash,
        &request.auth_verifier,
        &validated.wrapped_vault_key,
    )
    .await?;

    if rows_affected == 0 {
        Err(Unauthorized("User does not exist"))?;
    }

    let revoked_tokens = if request.revoke_other_sessions {
        Session::revoke_other_sessions(&mut *txn, user_id, token_hash.as_ref())
            .await?
    } else {
        Vec::new()
    };

    txn.commit().await?;

    for token in revoked_tokens {
        state.session_cache.invalidate(&token).await;
    }

    Ok(StatusCode::OK)
}
