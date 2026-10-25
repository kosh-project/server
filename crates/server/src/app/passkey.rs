use moka::future::Cache;
use uuid::Uuid;
use webauthn_rs::prelude::{DiscoverableAuthentication, PasskeyRegistration};

use crate::{
    api::{
        self,
        Error::{BadRequest, Unauthorized},
    },
    app::state::UserId,
};

#[derive(Clone)]
pub struct Passkey {
    pub registration_cache: Cache<Uuid, (UserId, PasskeyRegistration)>,
    pub auth_cache: Cache<Uuid, DiscoverableAuthentication>,
}

impl Passkey {
    pub async fn store_auth(
        &self,
        auth_state: DiscoverableAuthentication,
    ) -> Uuid {
        let challenge_id = Uuid::new_v4();
        self.auth_cache.insert(challenge_id, auth_state).await;

        challenge_id
    }

    /// Retrieves and removes a cached discoverable authentication challenge.
    ///
    /// # Errors
    /// Returns `BadRequest` if the challenge ID does not exist in cache or has expired.
    pub async fn take_auth(
        &self,
        id: &Uuid,
    ) -> api::Result<DiscoverableAuthentication> {
        self.auth_cache
            .remove(id)
            .await
            .ok_or(BadRequest("Challenge expired or invalid"))
    }

    pub async fn store_registration(
        &self,
        user_id: UserId,
        reg_state: PasskeyRegistration,
    ) -> Uuid {
        let challenge_id = Uuid::new_v4();
        self.registration_cache
            .insert(challenge_id, (user_id, reg_state))
            .await;
        challenge_id
    }

    /// Retrieves and removes a cached registration challenge for the expected user.
    ///
    /// # Errors
    /// - Returns `BadRequest` if the challenge ID does not exist in cache or has expired.
    /// - Returns `Unauthorized` if the cached challenge belongs to a different user.
    pub async fn take_registration(
        &self,
        id: &Uuid,
        expected_user_id: UserId,
    ) -> api::Result<PasskeyRegistration> {
        let (cached_user_id, reg_state) = self
            .registration_cache
            .remove(id)
            .await
            .ok_or(BadRequest("Challenge expired or invalid"))?;

        if cached_user_id != expected_user_id {
            return Err(Unauthorized("Mismatched user for challenge"));
        }

        Ok(reg_state)
    }
}
