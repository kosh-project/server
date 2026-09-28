use crate::{
    api::{self, Error::Unauthorized},
    auth::{Secret, TOKEN_LEN},
    error::internal,
    model::Result,
};
use std::time::{SystemTime, UNIX_EPOCH};

use blake3::{Hash, Hasher, keyed_hash};
use sqlx::SqlitePool;
use uuid::Uuid;

/// A row in the `sessions` table, representing an active authenticated session.
///
/// Sessions are identified by an opaque token (a UUID) that the client stores locally.
/// The server never stores the raw token — only its BLAKE3 hash. This means even if
/// the database is compromised, an attacker cannot reconstruct the original tokens.
///
/// Session lifetime is 30 days from creation. Expired sessions are lazily revoked
/// the next time the token is presented to the [`auth_guard`] middleware.
///
/// [`auth_guard`]: crate::api::middleware::auth_guard
#[derive(sqlx::FromRow)]
pub struct Session {
    pub token_hash: Vec<u8>,
    pub user_id: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

impl Session {
    /// Issues a new session for the given user and writes a record to the database.
    ///
    /// Generates a UUID v4 session ID, computes a 30-day expiry timestamp, and
    /// produces a bearer token in the format `session_id.expires_at.blake3_mac`.
    /// Only the BLAKE3 hash of the full token string is stored in the database;
    /// the raw token is returned to the caller exactly once and never persisted.
    ///
    /// # Errors
    ///
    /// - Returns a [`crate::model::error::Error`] wrapping [`sqlx::Error`] if the
    ///   database insert fails.
    /// - Returns an error if the system clock is set before the Unix epoch.
    /// - Returns an error if the current timestamp overflows an `i64` (year ~2262).
    pub async fn create(
        pool: &SqlitePool,
        user_id: i64,
        secret: &Secret,
    ) -> Result<String> {
        let session_id = Uuid::new_v4().to_string();

        // #[allow(clippy::as_conversions)]
        let created_at: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs()
            .try_into()?;

        let expires_at = created_at + (30 * 24 * 60 * 60);

        let payload = format!("{session_id}.{expires_at :016x}");
        let mac = keyed_hash(&secret.key(), payload.as_bytes());
        let token = format!("{payload}.{}", mac.to_hex());

        let token_hash = Hasher::new()
            .update(token.as_bytes())
            .finalize()
            .as_bytes()
            .to_vec();

        sqlx::query!(
            r#"
                INSERT INTO sessions (token_hash, user_id, created_at, expires_at)
                VALUES (?, ?, ?, ?)
                "#,
            token_hash,
            user_id,
            created_at,
            expires_at
        )
        .execute(pool)
        .await?;

        Ok(token)
    }

    /// Validates a bearer token's MAC and expiry without touching the database.
    ///
    /// This is the stateless pre-filter step (Layer 2 of the ingress funnel). It
    /// verifies the token format, recomputes the BLAKE3 MAC over the payload, and
    /// checks the embedded expiry timestamp — all in RAM. Any token that fails
    /// here was either never issued by this server or has expired, and no database
    /// lookup is needed to reject it.
    ///
    /// The token format is: `session_id(36).expires_at_hex(16).blake3_mac(64)`,
    /// for a total length of exactly [`TOKEN_LEN`] bytes.
    ///
    /// # Errors
    ///
    /// - `401 Unauthorized` — The token length is not [`TOKEN_LEN`], the BLAKE3
    ///   MAC does not match, or the token's expiry timestamp has passed.
    /// - `500 Internal Server Error` — The system clock is set before the Unix
    ///   epoch, or the timestamp overflows an `i64`.
    #[allow(clippy::string_slice)]
    pub fn verify_stateless(token: &str, secret: &Secret) -> api::Result<()> {
        if token.len() != TOKEN_LEN {

            return Err(api::Error::Unauthorized(
                "Invalid token format".into(),
            ));
        }

        let payload = &token[..53];
        let mac_hex = &token[54..];

        let received_mac = Hash::from_hex(mac_hex).map_err(|_| {
            api::Error::Unauthorized("Forged or invalid token".into())
        })?;

        let expected_mac =
            blake3::keyed_hash(&secret.key(), payload.as_bytes());

        if received_mac != expected_mac {
            return Err(api::Error::Unauthorized(
                "Forged or invalid token".into(),
            ));
        }

        let expires_hex = &token[37..53];
        let expires_at =
            i64::from_str_radix(expires_hex, 16).map_err(|_| {
                api::Error::Unauthorized("Invalid timestamp".into())
            })?;

        let now: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(internal::Error::from)?
            .as_secs()
            .try_into()?;

        if expires_at < now {
            return Err(Unauthorized("Token expired".into()));
        }

        Ok(())
    }

    /// Querries the database and returns [`Option<Session>`] wrapped in [`Result`].
    /// If any such token exists yields `Some(session)`
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] on failed querry to database.
    pub async fn verify(
        pool: &SqlitePool,
        token_hash: &[u8],
    ) -> Result<Option<Self>> {
        let session = sqlx::query_as!(
            Session,
            r#"
            SELECT
                token_hash as "token_hash!",
                user_id,
                created_at,
                expires_at
            FROM sessions WHERE token_hash = ?
        "#,
            token_hash
        )
        .fetch_optional(pool)
        .await?;

        let this_moment =
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

        if let Some(ref sess) = session
            && (this_moment < sess.created_at.try_into()?
                || this_moment > sess.expires_at.try_into()?)
        {
            Self::revoke(pool, token_hash).await?;
            return Ok(None);
        }

        Ok(session)
    }

    /// Removes the session entry from sessions entity.
    ///
    /// # Errors
    /// Fails with [`sqlx::Error`], if querrying with database fails
    pub async fn revoke(pool: &SqlitePool, token_hash: &[u8]) -> Result<()> {
        let _result = sqlx::query!(
            r#"
            DELETE FROM sessions WHERE token_hash = ?
        "#,
            token_hash
        )
        .execute(pool)
        .await?;

        Ok(())
    }
}

/// A fixed-size BLAKE3 hash of an opaque session token.
///
/// This newtype exists for two reasons:
///
/// 1. **Type safety:** It prevents raw byte slices from being used as cache keys by accident.
/// 2. **Zero-copy cache key:** Using `[u8; 32]` instead of `Vec<u8>` means the cache
///    key lives entirely on the stack with no heap allocation.
///
/// It implements `Hash + Eq` (required by `moka`) and `AsRef<[u8]>` for passing to
/// database queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenHash(pub [u8; 32]);

impl From<blake3::Hasher> for TokenHash {
    fn from(hasher: blake3::Hasher) -> Self {
        Self(hasher.finalize().into())
    }
}

impl From<blake3::Hash> for TokenHash {
    fn from(hash: blake3::Hash) -> Self {
        Self(hash.into())
    }
}

impl From<&[u8; 32]> for TokenHash {
    fn from(value: &[u8; 32]) -> Self {
        Self(*value)
    }
}

impl AsRef<[u8]> for TokenHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}
