use crate::api::Error::BadRequest;
use crate::api::auth::hashcash::HashCash;
use crate::app::State as AppState;
use crate::logger::Module;
use crate::model::{error::Error as ModelErr, user::User};
use crate::{Error as AppErr, Result, info};
use axum::Extension;
use axum::{Json, extract::State};
use hyper::StatusCode;
use serde::Deserialize;
use sqlx::Error as SqlErr;

/// The JSON body expected by the registration endpoint.
#[derive(Deserialize)]
pub struct Request {
    /// The authentication verifier derived from the user's credentials on the client side.
    ///
    /// The server stores this string verbatim and compares it on login. It is the
    /// client's responsibility to derive a strong verifier (e.g., using Argon2id)
    /// before sending it — the server does not perform any additional hashing.
    pub auth_verifier: String,
}

/// Registers a new user with the provided credentials.
///
/// # Errors
/// - Returns a `BadRequest` if the identity hash cannot be decoded from hex.
/// - Returns a `Conflict` if a user with the same identity hash already exists.
/// - Returns an internal error if a database query fails.
pub async fn register(
    State(state): State<AppState>,
    Extension(hashcash): Extension<HashCash>,
    Json(request): Json<Request>,
) -> Result<StatusCode> {
    let Ok(identity_hash) = hex::decode(&hashcash.identity_hash) else {
        Err(BadRequest("identity_hash failed to decode".into()))?
    };

    let result =
        User::create(&state.db, &identity_hash, request.auth_verifier).await;

    match result {
        Ok(()) => {
            info!(
                Module::Api,
                "New user registered with id: {:?}",
                hex::encode(identity_hash)
            );
            Ok(StatusCode::CREATED)
        }
        Err(ModelErr::Database(SqlErr::Database(err)))
            if err.is_unique_violation() =>
        {
            Err(AppErr::Conflict("User already exists".into()))
        }
        Err(e) => Err(e.into()),
    }
}
