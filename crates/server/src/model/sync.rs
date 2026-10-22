use crate::model::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceWatermark {
    pub device_id: Vec<u8>,
    pub user_id: i64,
    pub namespace: String,
    pub last_synced_seq: i64,
    pub last_seen_at: i64,
    pub is_revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerSnapshot {
    pub id: i64,
    pub user_id: i64,
    pub namespace: String,
    pub snapshot_seq: i64,
    pub blob_hash: Vec<u8>,
    pub created_at: i64,
}

impl DeviceWatermark {
    /// Upserts a watermark record for a device syncing a namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if executing the database query fails.
    pub async fn upsert(
        pool: &SqlitePool,
        device_id: &[u8],
        user_id: i64,
        namespace: &str,
        synced_seq: i64,
        now: i64,
    ) -> Result<()> {
        sqlx::query!(r#"
            INSERT INTO device_sync_watermarks (device_id, user_id, namespace, last_synced_seq, last_seen_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(device_id, user_id, namespace) DO UPDATE SET
                last_synced_seq = MAX(device_sync_watermarks.last_synced_seq, excluded.last_synced_seq),
                last_seen_at = excluded.last_seen_at
        "#,
        device_id,
        user_id,
        namespace,
        synced_seq,
        now
        ).execute(pool)
        .await?;

        Ok(())
    }

    /// Calculates the safe sequence threshold for pruning historical segments.
    ///
    /// # Errors
    ///
    /// Returns an error if executing the database query fails.
    pub async fn safe_prune_seq(
        pool: &SqlitePool,
        user_id: i64,
        namespace: &str,
        stale_cutoff: i64,
    ) -> Result<Option<i64>> {
        let row = sqlx::query!(
            r#"
            WITH active_devices AS (
                SELECT
                    MIN(last_synced_seq) AS seq
                FROM
                    device_sync_watermarks
                WHERE
                    user_id = ?1 AND
                    namespace = ?2 AND
                    is_revoked = 0 AND
                    last_seen_at >= ?3
            ),
            latest_snapshot AS (
                SELECT
                    MAX(snapshot_seq) AS seq
                FROM
                    ledger_snapshots
                WHERE
                    user_id = ?1 AND
                    namespace = ?2
            )
            SELECT
                active_devices.seq AS min_device_seq,
                latest_snapshot.seq AS max_snapshot_seq
            FROM
                active_devices,
                latest_snapshot
        "#,
            user_id,
            namespace,
            stale_cutoff
        )
        .fetch_one(pool)
        .await?;

        match (row.min_device_seq, row.max_snapshot_seq) {
            (Some(device_seq), Some(snapshot_seq)) => {
                Ok(Some(device_seq.min(snapshot_seq)))
            }
            _ => Ok(None),
        }
    }
}

impl LedgerSnapshot {
    /// Inserts a new snapshot record into the database.
    ///
    /// # Errors
    ///
    /// Returns an error if executing the database query fails.
    pub async fn create(
        pool: &SqlitePool,
        user_id: i64,
        namespace: &str,
        snapshot_seq: i64,
        blob_hash: &[u8],
        created_at: i64,
    ) -> Result<i64> {
        let id = sqlx::query!(
            r#"
            INSERT INTO ledger_snapshots
                (user_id, namespace, snapshot_seq, blob_hash, created_at)
            VALUES
                (?, ?, ?, ?, ?)
        "#,
            user_id,
            namespace,
            snapshot_seq,
            blob_hash,
            created_at
        )
        .execute(pool)
        .await?
        .last_insert_rowid();

        Ok(id)
    }

    /// Queries the latest snapshot record for a user and namespace.
    ///
    /// # Errors
    ///
    /// Returns an error if executing the database query fails.
    pub async fn latest(
        pool: &SqlitePool,
        user_id: i64,
        namespace: &str,
    ) -> Result<Option<Self>> {
        let record = sqlx::query_as!(
            Self,
            r#"
            SELECT
                id,
                user_id,
                namespace,
                snapshot_seq,
                blob_hash,
                created_at
            FROM
                ledger_snapshots
            WHERE
                user_id = ?1 AND
                namespace = ?2
            ORDER BY
                snapshot_seq DESC
            LIMIT 1
        "#,
            user_id,
            namespace
        )
        .fetch_optional(pool)
        .await?;

        Ok(record)
    }
}
