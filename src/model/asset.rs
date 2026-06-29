use crate::{Result, model::*, storage::file::Metadata};
use bytes::Bytes;
use sqlx::{SqlitePool, query};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
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

    pub async fn create(
        pool: &SqlitePool,
        user_id: i64,
        tag: AssetTag,
        metadata: &Metadata,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO assets (user_id, hash, size_bytes, last_modified, tag)
            VALUES(?, ?, ?, ?, ?)
            "#,
            user_id,
            metadata.hash.as_bytes().to_vec(),
            metadata.size,
            metadata.last_modified,
            tag
        ).execute(pool)
        .await?;
        Ok(())
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
