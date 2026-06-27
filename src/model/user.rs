use crate::api::Error as ApiErr;
use crate::error::{Error as AppErr, Result};
use sqlx::SqlitePool;

/// User entity for persistent storage
///
pub(crate) struct User {
    pub id: i64,
    pub identity_hash: [u8; 32],
    pub auth_verifier: String,
}

impl User {
    /// Verifies the user credentials by checking the database via SqlitePool
    pub async fn verify(
        pool: &SqlitePool,
        identity_hash: String,
        auth_verifier: String,
    ) -> Result<i64> {
        let hash = hex::decode(&identity_hash).or(Err(ApiErr::BadRequest(format!(
            "Couldn't decode {identity_hash}"
        ))))?;

        let result = sqlx::query!(
            r#"
                SELECT id FROM users WHERE identity_hash = ? AND auth_verifier = ?
            "#,
            hash,
            auth_verifier,
        )
        .fetch_optional(pool)
        .await?;

        Ok(result
            .ok_or(ApiErr::Unauthorized("Invalid credentials".into()))
            .map(|x| x.id)?)
    }
}
