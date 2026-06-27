use crate::api::Error as ApiErr;
use crate::error::{Error as AppErr, Result};
use sqlx::{Error as SqlErr, Executor, SqlitePool};

/// User entity for persistent storage
///
pub(crate) struct User {
    pub id: i64,
    pub identity_hash: [u8; 32],
    pub auth_verifier: String,
}

impl User {
    /// Inserts a new user to users entity via SqlitePool
    ///
    /// # Error
    /// - If querrying database causes failure, returns with [`sqlx::sqlite::SqliteQueryResult`]
    /// -
    pub async fn create(
        pool: &SqlitePool,
        identity_hash: Vec<u8>,
        auth_verifier: String,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO users (identity_hash, auth_verifier) VALUES(?, ?)
        "#,
            identity_hash,
            auth_verifier,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Verifies the credential pair (`identity_hash` and `auth_verifier`)
    /// stored in users entity via [`sqlx::SqlitePool`].
    /// Returns the user_id as `Some(i64)` wrapped in [`Result`]
    ///
    /// # Error
    /// - Returns with [`hex::FromHexError`] if identity_hash fails to decode into byte array.
    /// - Fails with [`sqlx::Error`], when querrying with database fails
    pub async fn verify(
        pool: &SqlitePool,
        identity_hash: Vec<u8>,
        auth_verifier: String,
    ) -> Result<Option<i64>> {
        let result = sqlx::query!(
            r#"
                SELECT id FROM users WHERE identity_hash = ? AND auth_verifier = ?
            "#,
            identity_hash,
            auth_verifier,
        )
        .fetch_optional(pool)
        .await?;

        Ok(result.map(|rec| rec.id))
    }
}
