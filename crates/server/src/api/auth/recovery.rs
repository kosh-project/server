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

#[derive(Deserialize)]
pub struct Request {
    pub identity_hash: String,
    pub auth_verifier: String,
    pub wrapped_vault_key: String,
    #[serde(default)]
    pub revoke_other_sessions: bool,
}

struct Validated {
    identity_hash: [u8; 32],
    wrapped_vault_key: Vec<u8>,
}

impl Request {
    fn decode(&self) -> Result<Validated> {
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
