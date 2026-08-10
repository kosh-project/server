use crate::model::Result;
use sqlx::SqlitePool;

/// Represents a registered user in the system.
///
/// Users are identified by an `identity_hash` (a hash of their public identity)
/// and authenticated via an `auth_verifier`. Neither field stores a raw password.
/// This keeps the server from ever knowing who the user actually is.
pub struct User {
    pub id: i64,
    pub identity_hash: [u8; 32],
    pub auth_verifier: String,
}

impl User {
    /// Inserts a new user into the `users` table via the provided `SqlitePool`.
    ///
    /// Both `identity_hash` and `auth_verifier` are expected to be pre-processed
    /// on the client side before being sent to this function.
    ///
    /// # Errors
    /// - Returns [`sqlx::Error::Database`] on a uniqueness violation, meaning a user
    ///   with the same `identity_hash` already exists.
    /// - Returns [`sqlx::Error`] if the database query fails for any other reason.
    pub async fn create(
        pool: &SqlitePool,
        identity_hash: &Vec<u8>,
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

    /// Verifies the credential pair (`identity_hash` and `auth_verifier`) against the database.
    ///
    /// Returns `Some(user_id)` if a matching user is found, or `None` if the
    /// credentials do not match any existing record. This is the primary authentication
    /// check used by the login endpoint.
    ///
    /// # Errors
    /// - Returns [`sqlx::Error`] if the database query fails.
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
