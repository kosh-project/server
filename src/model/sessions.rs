use crate::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use blake3::Hasher;
use sqlx::{Sqlite, SqlitePool};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct Sessions {
    pub token_hash: Vec<u8>,
    pub user_id: i64,
    pub created_at: i64,
    pub expires_at: i64,
}

impl Sessions {
    pub async fn create(pool: &SqlitePool, user_id: i64) -> Result<String> {
        let token = Uuid::new_v4().to_string();
        let mut hasher = Hasher::new();
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Diddy did what?")
            .as_secs() as i64;
        let expires_at = created_at + (30 * 24 * 60 * 60);

        hasher.update(token.as_bytes());
        let token_hash = Hasher::new().finalize().as_bytes().to_vec();

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
}
