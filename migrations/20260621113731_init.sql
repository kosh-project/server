-- User table to store users
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    identity_hash BLOB NOT NULL UNIQUE,
    auth_verifier TEXT NOT NULL
);


-- Asset Table (Dumb Bucket List)
CREATE TABLE IF NOT EXISTS assets (
    id BLOB PRIMARY KEY,
    user_id INTEGER NOT NULL,
    hash BLOB NOT NULL,
    size_bytes INTEGER NOT NULL,
    last_modified INTEGER NOT NULL,
    tag TEXT NOT NULL,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);


CREATE INDEX IF NOT EXISTS idx_assets_hash ON assets(hash);
