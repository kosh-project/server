use crate::Result;
use crate::error::internal::Error as InternalErr;
use axum::{Json, extract::State};
use chrono::Utc;
use serde::Serialize;
use webauthn_rs::prelude::{
    DiscoverableAuthentication, DiscoverableKey, Passkey, PublicKeyCredential,
};

use crate::{
    api::{Error::Unauthorized, auth::passkey::handle_to_user_id},
    app,
    error::JsonResult,
    model::{passkey, session::Session, user::User},
};

pub mod start {
    use serde::Serialize;
    use uuid::Uuid;
    use webauthn_rs::prelude::RequestChallengeResponse;

    #[derive(Serialize)]
    pub struct Response {
        pub challenge_id: Uuid,
        pub challenge: RequestChallengeResponse,
    }
}

pub mod finish {
    use serde::Deserialize;
    use uuid::Uuid;
    use webauthn_rs::prelude::PublicKeyCredential;

    #[derive(Deserialize)]
    pub struct Request {
        pub challenge_id: Uuid,
        pub credential: PublicKeyCredential,
    }
}

#[derive(Serialize)]
pub struct Response {
    pub token: String,
    pub wrapped_vault_key: String,
}

pub async fn login_start(
    State(state): State<app::State>,
) -> JsonResult<start::Response> {
    let (challenge, auth_state) =
        state.webauthn.start_discoverable_authentication()?;

    let challenge_id = state.passkey.store_auth(auth_state).await;

    Ok(Json(start::Response {
        challenge_id,
        challenge,
    }))
}

pub async fn login_finish(
    State(state): State<app::State>,
    Json(request): Json<finish::Request>,
) -> JsonResult<Response> {
    let auth_state = state.passkey.take_auth(&request.challenge_id).await?;

    let (user_handle, cred_id) = state
        .webauthn
        .identify_discoverable_authentication(&request.credential)?;
    let expected_user_id = handle_to_user_id(&user_handle)?;

    let record = passkey::Record::find(&state.db, cred_id)
        .await?
        .ok_or(Unauthorized("Unknown passkey credential"))?;

    if record.user_id != expected_user_id {
        return Err(Unauthorized("Passkey user handle mismatch").into());
    }

    verify_and_update_passkey(&state, &record, &request.credential, auth_state)
        .await?;

    let user = User::find_by_id(&state.db, record.user_id)
        .await?
        .ok_or(Unauthorized("User record not found"))?;

    let token =
        Session::create(&state.db, record.user_id, &state.secret).await?;

    Ok(Json(Response {
        token,
        wrapped_vault_key: hex::encode(user.wrapped_vault_key),
    }))
}

pub async fn verify_and_update_passkey(
    state: &app::State,
    record: &passkey::Record,
    credential: &PublicKeyCredential,
    auth_state: DiscoverableAuthentication,
) -> Result<()> {
    let mut passkey: Passkey = serde_json::from_slice(&record.passkey_data)
        .map_err(|_| {
            InternalErr::Message("Corrupted passkey state in database")
        })?;

    let discoverable_key = DiscoverableKey::from(&passkey);
    let auth_result = state.webauthn.finish_discoverable_authentication(
        credential,
        auth_state,
        &[discoverable_key],
    )?;

    passkey.update_credential(&auth_result);
    let updated_data = serde_json::to_vec(&passkey).map_err(|_| {
        InternalErr::Message("Failed to serialize updated passkey")
    })?;
    let new_counter = i64::from(auth_result.counter());
    let now = Utc::now().timestamp();

    passkey::Record::update_counter(
        &state.db,
        &record.credential_id,
        &updated_data,
        new_counter,
        now,
    )
    .await?;

    Ok(())
}
