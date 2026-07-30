use crate::{
    Result, model::session::TokenHash, storage::file::Metadata,
};
use sqlx::{SqlitePool, pool, query};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
#[allow(unused)]
pub struct Asset {
    uuid: Uuid,
    hash: Vec<u8>,
    last_modified: i64,
    user: i64,
    size: i64,
    tag: AssetTag,
}

#[derive(sqlx::Type, Copy, Clone)]
#[repr(i32)]
pub enum AssetTag {
    GalleryMeta = 0,
    GalleryItem = 1,
    DriveMeta = 2,
    DriveItem = 3,
}

impl Asset {
    pub async fn exists(
        pool: &SqlitePool,
        hash: Vec<u8>,
    ) -> Result<bool> {
        let result = query!(
            "SELECT 1 AS matched FROM assets WHERE hash = ?",
            hash
        )
        .fetch_optional(pool)
        .await?;

        Ok(result.is_some())
    }

    /// Registers an asset entry to the assets entity
    ///
    /// Error
    /// - Returns [`sqlx::Error::Database`] for uniqueness violation,
    /// because of `uuid` being PRIMARY KEY, which happens very (very) rarely.
    /// - Otherwise, this can fail on querrying databases
    pub async fn create(
        pool: &SqlitePool,
        user: i64,
        tag: AssetTag,
        metadata: &Metadata,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO assets (id, user_id, hash, size_bytes, last_modified,tag)
            VALUES(?, ?, ?, ?, ?, ?)
            "#,
            Uuid::new_v4().as_bytes().to_vec(),
            user,
            metadata.hash.as_bytes().to_vec(),
            metadata.size,
            metadata.last_modified,
            tag
        ).execute(pool)
        .await?;

        Ok(())
    }

    /// Deletes user's ownership over an asset
    /// - Returns `Ok(true)`, if no other user owns same asset. Good signal to wipe that asset physically off the server.
    /// - Return `Ok(false)`, if user doesn't own the file, or other users have own the same file.
    pub async fn delete(
        pool: &SqlitePool,
        user: i64,
        hash: &[u8],
    ) -> Result<bool> {
        let mut tx = pool.begin().await?;

        let result = sqlx::query!(
            r#"
            DELETE FROM assets WHERE user_id = ? AND hash = ?
        "#,
            user,
            hash
        )
        .execute(&mut *tx)
        .await?;

        if result.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(false);
        }

        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM assets WHERE hash = ? ",
            hash
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(count == 0)
    }

    pub async fn owned_by(
        pool: &SqlitePool,
        user: i64,
        hash: &[u8],
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
            SELECT 1 AS matched FROM assets WHERE user_id = ? AND hash = ?
        "#,
            user,
            hash,
        ).fetch_optional(pool)
        .await?;

        Ok(result.is_some())
    }
}

impl TryFrom<&str> for AssetTag {
    type Error = ();
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        use AssetTag::*;

        Ok(match value {
            "0" => GalleryMeta,
            "1" => GalleryItem,
            "2" => DriveMeta,
            "3" => DriveItem,
            _ => Err(())?,
        })
    }
}
