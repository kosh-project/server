mod page;
mod tag;

#[cfg(test)]
mod test;

pub use page::{AssetCursor, MetadataRow, Page};
pub use tag::Tag;

use crate::{model::error::Result, storage::file::Metadata};
use sqlx::{SqlitePool, query};
use uuid::Uuid;

/// A record in the `assets` table representing one user's ownership of a blob.
#[derive(sqlx::FromRow)]
#[allow(unused)]
pub struct Asset {
    uuid: Uuid,
    hash: Vec<u8>,
    last_modified: i64,
    user: i64,
    size: i64,
    tag: Tag,
}

impl Asset {
    /// Returns whether asset exists with given hash
    pub async fn exists(pool: &SqlitePool, hash: Vec<u8>) -> Result<bool> {
        let result =
            query!("SELECT 1 AS matched FROM assets WHERE hash = ?", hash)
                .fetch_optional(pool)
                .await?;

        Ok(result.is_some())
    }

    /// Registers an asset entry to the assets entity
    pub async fn create(
        pool: &SqlitePool,
        user: i64,
        tag: Tag,
        metadata: &Metadata,
    ) -> Result<()> {
        sqlx::query!(
                r#"
                INSERT INTO assets (id, user_id, hash, size_bytes, last_modified, tag)
                VALUES(?, ?, ?, ?, ?, ?)
                "#,
                Uuid::new_v4().as_bytes().to_vec(),
                user,
                metadata.hash.as_bytes().to_vec(),
                metadata.size,
                metadata.last_modified,
                tag
            )
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Deletes user's ownership over an asset
    pub async fn delete(
        pool: &SqlitePool,
        user: i64,
        hash: &[u8],
    ) -> Result<()> {
        sqlx::query!(
            r#"
                DELETE FROM assets WHERE user_id = ? AND hash = ?
                "#,
            user,
            hash
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Checks if any [`Asset`] with provided `hash` exists, and is owned by the specified `user`.
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
        )
        .fetch_optional(pool)
        .await?;

        Ok(result.is_some())
    }

    /// Returns a paginated list of asset metadata rows for a given user using keyset cursor pagination.
    ///
    /// Fetches up to `limit` rows. Queries for `limit + 1` rows to determine whether
    /// a next page exists without issuing a secondary `COUNT(*)` query.
    pub async fn list(
        pool: &SqlitePool,
        user_id: i64,
        tag_filter: Tag,
        limit: u32,
        cursor: Option<&AssetCursor>,
    ) -> Result<Page> {
        let cursor_time = cursor.map(|c| c.before_time);
        let cursor_id = cursor.map(AssetCursor::decode_id).transpose()?;
        let fetch_limit = i64::from(limit.max(1)).saturating_add(1);

        let mut rows = sqlx::query!(
            r#"
                SELECT
                    id as "id!",
                    hash,
                    size_bytes,
                    last_modified,
                    tag as "tag: Tag"
                FROM assets
                WHERE user_id = ?1
                    AND tag = ?2
                    AND (
                        ?3 IS NULL
                        OR last_modified < ?3
                        OR (last_modified = ?3 AND id < ?4)
                    )
                ORDER BY last_modified DESC, id DESC
                LIMIT ?5
                "#,
            user_id,
            tag_filter,
            cursor_time,
            cursor_id,
            fetch_limit
        )
        .fetch_all(pool)
        .await?;

        let take_count = usize::try_from(limit).unwrap_or(0);
        let has_more = rows.len() > take_count;

        if has_more {
            rows.truncate(take_count);
        }

        let next_cursor = if has_more {
            rows.last().map(|row| AssetCursor {
                before_time: row.last_modified,
                before_id: hex::encode(&row.id),
            })
        } else {
            None
        };

        let assets = rows
            .into_iter()
            .map(|row| MetadataRow {
                hash: hex::encode(row.hash),
                size_bytes: row.size_bytes,
                last_modified: row.last_modified,
                tag: row.tag,
            })
            .collect();

        Ok(Page {
            assets,
            next_cursor,
        })
    }
}
