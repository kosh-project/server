use crate::model::*;
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

#[derive(sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
pub enum AssetTag {
    GalleryMeta,
    GalleryItem,
    DriveMeta,
    DriveItem,
}

impl Asset {
    pub async fn exists(pool: &SqlitePool, hash: Vec<u8>) -> Result<bool> {
        let result = query!("SELECT 1 AS matched FROM assets WHERE hash = ?", hash)
            .fetch_optional(pool)
            .await?;

        Ok(result.is_some())
    }
}
