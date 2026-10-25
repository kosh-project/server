use crate::Result;
use axum::{Extension, Json, extract::State};
use hyper::StatusCode;

use crate::{
    api::auth::passkey::user_id_to_handle, app, error::JsonResult,
    model::passkey,
};

pub mod start {
    use serde::Serialize;
    use uuid::Uuid;
    use webauthn_rs::prelude::CreationChallengeResponse;

    #[derive(Serialize)]
    pub struct Response {
        pub challenge_id: Uuid,
        pub challenge: CreationChallengeResponse,
    }
}

pub mod finish {
    use serde::Deserialize;
    use uuid::Uuid;
    use webauthn_rs::prelude::RegisterPublicKeyCredential;

    use crate::api::{self, Error::BadRequest};

    #[derive(Deserialize)]
    pub struct Request {
        pub challenge_id: Uuid,
        pub credential: RegisterPublicKeyCredential,
        pub encrypted_metadata: String,
    }

    impl Request {
        pub fn parse_metadata(&self) -> api::Result<Vec<u8>> {
            if self.encrypted_metadata.len() > 2048 {
                return Err(BadRequest(
                    "encrypted_metadata can't exceed 2048 hex chars (or 1024 bytes)",
                ));
            }

            let Ok(bytes) = hex::decode(&self.encrypted_metadata) else {
                return Err(BadRequest("Invalid hex encrypted_metadata"))?;
            };

            Ok(bytes)
        }
    }
}

pub async fn register_start(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
) -> JsonResult<start::Response> {
    let exclude_credentials =
        passkey::Record::exclude_list_for_user(&state.db, user_id).await?;

    let user_handle = user_id_to_handle(user_id)?;

    let (challenge, reg_state) = state.webauthn.start_passkey_registration(
        user_handle,
        "kosh-user",
        "Kosh Vault User",
        exclude_credentials,
    )?;

    let challenge_id =
        state.passkey.store_registration(user_id, reg_state).await;

    Ok(Json(start::Response {
        challenge_id,
        challenge,
    }))
}

pub async fn register_finish(
    State(state): State<app::State>,
    Extension(user_id): Extension<i64>,
    Json(request): Json<finish::Request>,
) -> Result<StatusCode> {
    let reg_state = state
        .passkey
        .take_registration(&request.challenge_id, user_id)
        .await?;

    let encrypted_metadata = request.parse_metadata()?;

    let passkey = state
        .webauthn
        .finish_passkey_registration(&request.credential, &reg_state)?;

    let record = passkey::Record::new(user_id, &passkey, encrypted_metadata)?;
    passkey::Record::create(&state.db, &record).await?;

    Ok(StatusCode::CREATED)
}
