CREATE TABLE IF NOT EXISTS device_sync_watermarks (
    device_id BLOB NOT NULL,
    user_id INTEGER NOT NULL,
    namespace TEXT NOT NULL,
    last_synced_seq INTEGER NOT NULL DEFAULT 0,
    last_seen_at INTEGER NOT NULL,
    is_revoked INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (device_id, user_id, namespace)
);

CREATE INDEX IF NOT EXISTS idx_watermarks_active
ON device_sync_watermarks(user_id, namespace, is_revoked, last_seen_at);

CREATE TABLE IF NOT EXISTS ledger_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    namespace TEXT NOT NULL,
    snapshot_seq INTEGER NOT NULL,
    blob_hash BLOB NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_snapshots_lookup
ON ledger_snapshots(user_id, namespace, snapshot_seq DESC);
