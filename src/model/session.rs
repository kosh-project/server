use crate::{Result, model::session};
use std::time::{SystemTime, UNIX_EPOCH};

use blake3::Hasher;
use sqlx::SqlitePool;
use uuid::Uuid;

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
    /// # Error
    /// Fails with [`sqlx::sqlite::SqliteQueryResult`] if an error occurs interacting with sqlite database
    ///
    /// # Panic
    /// This function panics if system time is set earlier than [`UNIX_EPOCH`] \
    /// To know more about this, see [`SystemTime::duration_since`]
    pub async fn create(pool: &SqlitePool, user_id: i64) -> Result<String> {
        let token = Uuid::new_v4().to_string();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Diddy did what?")
            .as_secs() as i64;
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

    pub async fn verify(pool: &SqlitePool, token_hash: &[u8]) -> Result<Option<Self>> {
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

        Ok(session)
    }
}
