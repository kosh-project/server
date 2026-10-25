//! In-memory challenge caches for multi-step `WebAuthn` ceremonies.
//!
//! Both `WebAuthn` registration and authentication follow a two-step challenge-response
//! protocol:
//!
//! 1. **Ceremony start:** The server generates a cryptographic challenge along with an
//!    intermediate verification state, saves it in an in-memory cache, and hands an
//!    ephemeral challenge ID to the client.
//! 2. **Ceremony finish:** The client submits the authenticator response along with the
//!    challenge ID. The server retrieves and immediately removes the challenge state
//!    from the cache, then cryptographically verifies the authenticator's assertion.
//!
//! To prevent replay attacks, challenge entries are single-use (`take_*` removes them
//! atomically). To prevent memory exhaustion and stale challenges, entries expire
//! automatically via a bounded Time-To-Live (`TTL`) managed by [`moka::future::Cache`].

use std::time::Duration;

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

/// Thread-safe in-memory cache holding pending `WebAuthn` ceremony states.
///
/// Cloning this struct is an O(1) operation because [`Cache`] uses reference-counted
/// shared ownership internally. Both caches are configured with a 60-second `TTL`
/// and a maximum capacity of 1,000 active challenges.
#[derive(Clone)]
pub struct Passkey {
    /// Pending registration ceremonies, indexed by challenge ID.
    ///
    /// Stores the tuple `(UserId, PasskeyRegistration)` so that the finishing step can
    /// verify the user completing registration is identical to the user who started it.
    pub registration_cache: Cache<Uuid, (UserId, PasskeyRegistration)>,

    /// Pending discoverable authentication ceremonies, indexed by challenge ID.
    ///
    /// Stores the [`DiscoverableAuthentication`] state generated during login initiation.
    pub auth_cache: Cache<Uuid, DiscoverableAuthentication>,
}

impl Default for Passkey {
    /// Creates a new `Passkey` challenge cache with default capacity and TTL limits.
    ///
    /// Configures both registration and authentication caches with a 60-second Time-To-Live
    /// (TTL) and a 1,000-entry maximum capacity.
    fn default() -> Self {
        let registration_cache = Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .max_capacity(1_000)
            .build();

        let auth_cache = Cache::builder()
            .time_to_live(Duration::from_secs(60))
            .max_capacity(1_000)
            .build();

        Self {
            registration_cache,
            auth_cache,
        }
    }
}

impl Passkey {
    /// Constructs a new `Passkey` challenge cache with default TTL (60 seconds) and capacity (1,000 entries).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a pending discoverable authentication state and returns its unique challenge ID.
    ///
    /// Generates a fresh random version 4 [`Uuid`] as the lookup key. The state will
    /// automatically expire after the configured `TTL` (60 seconds) if not claimed.
    pub async fn store_auth(
        &self,
        auth_state: DiscoverableAuthentication,
    ) -> Uuid {
        let challenge_id = Uuid::new_v4();
        self.auth_cache.insert(challenge_id, auth_state).await;

        challenge_id
    }

    /// Retrieves and atomically removes a cached discoverable authentication challenge.
    ///
    /// Because the entry is removed immediately, the challenge cannot be replayed or reused.
    ///
    /// # Errors
    ///
    /// - Returns [`BadRequest`] if the challenge ID does not exist in the cache or has expired past its `TTL`.
    pub async fn take_auth(
        &self,
        id: &Uuid,
    ) -> api::Result<DiscoverableAuthentication> {
        self.auth_cache
            .remove(id)
            .await
            .ok_or(BadRequest("Challenge expired or invalid"))
    }

    /// Stores a pending passkey registration state bound to a user ID.
    ///
    /// Generates a fresh random version 4 [`Uuid`] as the lookup key. The state will
    /// automatically expire after the configured `TTL` (60 seconds) if not claimed.
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

    /// Retrieves and atomically removes a cached registration challenge for the expected user.
    ///
    /// Validates that the user ID requesting completion matches the user ID that initiated
    /// the ceremony. If the user IDs do not match, the entry is already consumed to prevent
    /// further exploitation, and an error is returned.
    ///
    /// # Errors
    ///
    /// - Returns [`BadRequest`] if the challenge ID does not exist in the cache or has expired past its `TTL`.
    /// - Returns [`Unauthorized`] if the cached challenge belongs to a different user than `expected_user_id`.
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
