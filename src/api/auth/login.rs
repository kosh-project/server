use crate::error::Result;
use crate::model::session::Session;
use crate::model::user::User;
use crate::{
    api::Error::{BadRequest, Unauthorized},
    app::State as AppState,
};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    identity_hash: String,
    auth_verifier: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<LoginResponse>> {
    let Ok(identity_hash) = hex::decode(&request.identity_hash) else {
        Err(BadRequest("identity_hash failed to decode".into()))?
    };

    let user_id =
        User::verify(&state.db, identity_hash, request.auth_verifier)
            .await?
            .ok_or(Unauthorized("Invalid Credentials".into()))?;

    let token = Session::create(&state.db, user_id).await?;

    Ok(Json(LoginResponse { token }))
}
