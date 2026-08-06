use crate::model::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use blake3::Hasher;
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
    /// Creates a new entry in `Sessions` entity then returns a hashed token as String
    ///
    /// # Errors
    /// - Fails with [`sqlx::sqlite::SqliteQueryResult`] if an error occurs interacting with sqlite database.
    /// - Returns an error if system time is set earlier than [`UNIX_EPOCH`].
    pub async fn create(
        pool: &SqlitePool,
        user_id: i64,
    ) -> Result<String> {
        let token = Uuid::new_v4().to_string();

        #[allow(clippy::as_conversions)]
        // Happens only when you mess up with your system time
        let created_at: i64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs()
            .try_into()?;

        let expires_at = created_at + (30 * 24 * 60 * 60);

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
    pub async fn revoke(
        pool: &SqlitePool,
        token_hash: &[u8],
    ) -> Result<()> {
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
