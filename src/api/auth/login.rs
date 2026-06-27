use crate::error::Result;
use crate::model::sessions::Sessions;
use crate::model::user::User;
use crate::{api::Error, app::State as AppState};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(crate) struct LoginResponse {
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
    let user_id = User::verify(&state.db, request.identity_hash, request.auth_verifier).await?;

    let token = Sessions::create(&state.db, user_id).await?;

    Ok(Json(LoginResponse { token }))
}
