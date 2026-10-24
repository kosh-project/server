use crate::model::Result;
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub credential_id: Vec<u8>,
    pub user_id: i64,

    pub passkey_data: Vec<u8>,
    pub counter: i64,

    pub encrypted_metadata: Vec<u8>,

    pub created_at: i64,
    pub last_used: i64,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub credential_id: String,
    pub encrypted_metadata: String,
    pub created_at: i64,
    pub last_used: i64,
}

impl Record {
    pub async fn create(pool: &SqlitePool, record: &Self) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT
                INTO webauthn_credentials (
                    credential_id,
                    user_id,
                    encrypted_metadata,
                    passkey_data,
                    counter,
                    created_at,
                    last_used
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
            record.credential_id,
            record.user_id,
            record.encrypted_metadata,
            record.passkey_data,
            record.counter,
            record.created_at,
            record.last_used
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn find(pool: &SqlitePool, id: &[u8]) -> Result<Option<Self>> {
        let record = sqlx::query_as!(
            Self,
            r#"
            SELECT
                credential_id AS 'credential_id!',
                user_id,
                encrypted_metadata,
                passkey_data AS 'passkey_data!',
                counter,
                created_at,
                last_used
            FROM
                webauthn_credentials
            WHERE
                credential_id = ?
        "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }

    pub async fn list_for_user(
        pool: &SqlitePool,
        user_id: i64,
    ) -> Result<Vec<Summary>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                credential_id AS 'credential_id!',
                encrypted_metadata,
                created_at,
                last_used
            FROM
                webauthn_credentials
            WHERE
                user_id = ?
            ORDER BY
                created_at DESC
        "#,
            user_id
        )
        .fetch_all(pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Summary {
                credential_id: hex::encode(r.credential_id),
                encrypted_metadata: hex::encode(r.encrypted_metadata),
                created_at: r.created_at,
                last_used: r.last_used,
            })
            .collect())
    }

    pub async fn update_counter(
        pool: &SqlitePool,
        credential_id: &[u8],
        new_counter: i64,
        last_used: i64,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE
                webauthn_credentials
            SET
                counter = ?,
                last_used = ?
            WHERE
                credential_id = ?
        "#,
            new_counter,
            last_used,
            credential_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub async fn delete(
        pool: &SqlitePool,
        credential_id: &[u8],
        user_id: i64,
    ) -> Result<bool> {
        let rows_affected = sqlx::query!(
            r#"
            DELETE
            FROM
                webauthn_credentials
            WHERE
                credential_id = ? AND
                user_id = ?
        "#,
            credential_id,
            user_id
        )
        .execute(pool)
        .await?
        .rows_affected();

        Ok(rows_affected > 0)
    }
}
