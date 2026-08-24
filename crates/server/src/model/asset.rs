use crate::{model::error::Result, storage::file::Metadata};
use sqlx::{SqlitePool, query};
use uuid::Uuid;

/// A record in the `assets` table representing one user's ownership of a blob.
///
/// The server stores files by their BLAKE3 hash (CAS semantics). Multiple users
/// can upload identical files — each gets their own `Asset` row pointing to the
/// same hash. When a user deletes their asset, only their row is removed. The
/// physical blob is only deleted when no `Asset` rows reference its hash anymore.
/// This mirrors Unix hard-link semantics.
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

/// Classifies an asset into a logical section of the Android client.
///
/// Stored as an `i32` in `SQLite` to minimize storage footprint. The client
/// uses this tag to route an asset to the correct UI view (gallery or drive)
/// without needing to inspect the encrypted file contents.
#[derive(sqlx::Type, Copy, Clone)]
#[repr(i32)]
pub enum AssetTag {
    /// Metadata file for the Gallery section (encrypted index, thumbnails, etc.).
    GalleryMeta = 0,
    /// An encrypted media file belonging to the Gallery section.
    GalleryItem = 1,
    /// Metadata file for the Drive section.
    DriveMeta = 2,
    /// An encrypted file belonging to the Drive section.
    DriveItem = 3,
}

impl Asset {
    /// Returns whether asset exists with given hash
    ///
    /// # Errors
    /// - Fails with [`sqlx::Error`] if fails to work with database
    pub async fn exists(pool: &SqlitePool, hash: Vec<u8>) -> Result<bool> {
        let result =
            query!("SELECT 1 AS matched FROM assets WHERE hash = ?", hash)
                .fetch_optional(pool)
                .await?;

        Ok(result.is_some())
    }

    /// Registers an asset entry to the assets entity
    ///
    /// # Errors
    /// - Returns [`sqlx::Error::Database`] for uniqueness violation,
    ///   because of `uuid` being PRIMARY KEY, which happens very (very) rarely.
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
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] if a database transaction fails.
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

    /// Checks if any [`Asset`] with provided `hash` exists, and is owned by the specified `user` as well
    ///
    /// It's different from [`Asset::exists`], because it checks for ownership.
    /// Required for situations where user tries to access an asset, this returns the proof of it.
    ///
    /// # Errors
    /// Returns [`sqlx::Error`] if the database query fails.
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
}

impl TryFrom<&str> for AssetTag {
    type Error = ();
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        use AssetTag::{DriveItem, DriveMeta, GalleryItem, GalleryMeta};

        Ok(match value {
            "0" => GalleryMeta,
            "1" => GalleryItem,
            "2" => DriveMeta,
            "3" => DriveItem,
            _ => Err(())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::model::asset::Asset;

    async fn setup_db() -> Result<SqlitePool> {
        let pool = SqlitePool::connect("sqlite::memory:").await?;
        sqlx::migrate!().run(&pool).await?;
        Ok(pool)
    }

    async fn insert_asset(
        pool: &SqlitePool,
        user_id: i64,
        hash: &[u8],
    ) -> Result<()> {
        let dummy_id = format!("id_hash_{user_id}");

        sqlx::query!(r#"
            INSERT OR IGNORE INTO users (id, identity_hash, auth_verifier) VALUES (?, ?, ?)
        "#,
        user_id,
        dummy_id,
        "dummy_verifier"
        ).execute(pool)
        .await?;

        sqlx::query!(r#"
            INSERT INTO assets (id, user_id, hash, size_bytes, last_modified, tag) VALUES (?, ?, ?, ?, ?, ?)
            "#,
            Uuid::new_v4().as_bytes().to_vec(),
            user_id,
            hash,
            100,
            0,
            0
        ).execute(pool)
        .await?;
        Ok(())
    }

    async fn count_owners(pool: &SqlitePool, hash: &[u8]) -> Result<i64> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM assets WHERE hash = ?",
            hash as &[u8]
        )
        .fetch_one(pool)
        .await?;

        Ok(count)
    }

    #[tokio::test]
    async fn delete_asset_with_single_owner() -> Result<()> {
        let pool = setup_db().await?;
        let hash = b"hello_fellas_i_m_deleting_a_file";

        insert_asset(&pool, 10, hash).await?;

        Asset::delete(&pool, 10, hash).await?;

        let count = count_owners(&pool, hash).await?;

        // Test: No asset should be present as there were no other owners
        assert_eq!(count, 0);

        Ok(())
    }

    #[tokio::test]
    async fn attempt_to_delete_unowned_asset() -> Result<()> {
        let pool = setup_db().await?;
        let hash = b"a_dude_uploads_a_file_with_cache";

        insert_asset(&pool, 10, hash).await?;

        Asset::delete(&pool, 12, hash).await?;

        let count = count_owners(&pool, hash).await?;

        // Test: owner_count shouldn't be affected by this
        assert_eq!(count, 1);

        Ok(())
    }
}
