use crate::model::Result;
use sqlx::{SqliteExecutor, SqlitePool};

/// Represents a registered user in the system.
///
/// Users are identified by an `identity_hash` (a hash of their public identity)
/// and authenticated via an `auth_verifier`. Neither field stores a raw password.
/// This keeps the server from ever knowing who the user actually is.
pub struct User {
    /// Unique auto-incrementing database identifier (`users.id`).
    pub id: i64,
    /// 32-byte BLAKE3 identity hash representing the user's public identity.
    pub identity_hash: [u8; 32],
    /// Argon2id authentication verifier string used to validate login attempts.
    pub auth_verifier: String,
    /// 60-byte ChaCha20-Poly1305 wrapped root vault key.
    pub wrapped_vault_key: Vec<u8>,
}

/// Represents the minimal verified user credentials returned upon successful login verification.
#[derive(Debug, sqlx::FromRow, PartialEq, Eq, Clone)]
pub struct Verified {
    /// Unique database identifier (`users.id`).
    pub id: i64,
    /// 60-byte ChaCha20-Poly1305 wrapped root vault key.
    pub wrapped_vault_key: Vec<u8>,
}

impl User {
    /// Inserts a new user into the `users` table via the provided `SqlitePool`.
    ///
    /// Both `identity_hash` and `auth_verifier` are expected to be pre-processed
    /// on the client side before being sent to this function.
    ///
    /// # Errors
    /// - Returns [`crate::model::Error::UniqueViolation`] if a user with the same `identity_hash` already exists.
    /// - Returns [`crate::model::Error::Database`] if the database query fails.
    pub async fn create(
        pool: &SqlitePool,
        identity_hash: &[u8],
        auth_verifier: String,
        wrapped_vault_key: &[u8],
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                users (
                    identity_hash,
                    auth_verifier,
                    wrapped_vault_key
            ) VALUES ( ?, ?, ?)
        "#,
            identity_hash,
            auth_verifier,
            wrapped_vault_key
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Verifies the credential pair (`identity_hash` and `auth_verifier`) against the database.
    ///
    /// Returns `Some(Verified)` containing user ID and wrapped vault key if a matching user is found,
    /// or `None` if the credentials do not match any existing record.
    ///
    /// # Errors
    /// - Returns [`crate::model::Error::Database`] if the database query fails.
    pub async fn verify(
        pool: &SqlitePool,
        identity_hash: &[u8],
        auth_verifier: String,
    ) -> Result<Option<Verified>> {
        let result = sqlx::query_as!(
            Verified,
            r#"
                SELECT
                    id,
                    wrapped_vault_key
                FROM
                    users
                WHERE
                    identity_hash = ? AND
                    auth_verifier = ?
            "#,
            identity_hash,
            auth_verifier,
        )
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }

    /// Atomically updates a user's `identity_hash`, `auth_verifier`, and `wrapped_vault_key`.
    ///
    /// Returns the number of rows affected (1 if the user existed and was updated, 0 otherwise).
    ///
    /// # Errors
    /// - Returns [`crate::model::Error::UniqueViolation`] if the new `identity_hash` is already claimed.
    /// - Returns [`crate::model::Error::Database`] if the database query fails.
    pub async fn rotate_recovery<'e, E>(
        pool: E,
        user_id: i64,
        identity_hash: &[u8],
        auth_verifier: &str,
        wrapped_vault_key: &[u8],
    ) -> Result<u64>
    where
        E: SqliteExecutor<'e>,
    {
        let rows_affected = sqlx::query!(
            r#"
            UPDATE
                users
            SET
                identity_hash = ?,
                auth_verifier = ?,
                wrapped_vault_key = ?
            WHERE
                id = ?
        "#,
            identity_hash,
            auth_verifier,
            wrapped_vault_key,
            user_id
        )
        .execute(pool)
        .await?
        .rows_affected();

        Ok(rows_affected)
    }

    /// Looks up a verified user record by database primary key ID.
    ///
    /// # Errors
    /// Returns an error if the database query fails.
    pub async fn find_by_id(
        pool: &SqlitePool,
        id: i64,
    ) -> Result<Option<Verified>> {
        let result = sqlx::query_as!(
            Verified,
            r#"
                SELECT
                    id,
                    wrapped_vault_key
                FROM
                    users
                WHERE
                    id = ?
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(result)
    }
}
