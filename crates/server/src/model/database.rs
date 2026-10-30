//! SQLite database connection pool setup, performance PRAGMA tuning, and schema migration runner.
//!
//! Kosh stores metadata (users, sessions, asset ownership records, passkey credentials, and sync watermarks)
//! in an embedded `SQLite` database located at `<vault_path>/metadata.db`.
//!
//! To achieve high read concurrency and robust transaction durability on flash storage:
//! - **Write-Ahead Logging (`WAL`)**: Readers never block writers, and writers never block readers.
//! - **`synchronous = NORMAL`**: Safe with WAL mode while eliminating unnecessary fsync overhead on spinning/flash drives.
//! - **Memory-mapped I/O (`mmap_size`)**: Enables direct memory-mapped paging up to 30 GB, bypassing userspace buffer copying.
//! - **In-memory temp store (`temp_store = MEMORY`)**: Avoids temporary file creation during sorting and indexing.
//! - **Foreign key enforcement (`foreign_keys = true`)**: Enforces relational constraints (e.g. cascading session invalidation).

use std::str::FromStr;

use crate::{error::boot::Result, info};
use kosh_core::{config::Config, logger::Module};
use sqlx::{
    SqlitePool,
    sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};

/// Initializes the SQLite metadata database pool, configures high-performance PRAGMAs, and applies pending migrations.
///
/// Constructs the connection string targeting `<vault_path>/metadata.db`, applies production
/// PRAGMAs (WAL journal mode, Normal synchronous mode, 30 GB mmap size, in-memory temp store,
/// foreign key cascades), builds a connection pool with up to 50 concurrent connections,
/// and executes all pending SQL schema migrations from `./migrations`.
///
/// # Arguments
///
/// * `config` - The loaded server configuration containing `vault_path`.
///
/// # Errors
///
/// - Returns [`crate::error::boot::Error::Database`] if connecting to SQLite fails.
/// - Returns [`crate::error::boot::Error::Migration`] if applying pending schema migrations fails.
pub async fn init(config: &Config) -> Result<SqlitePool> {
    let db_path =
        format!("sqlite://{}/metadata.db", config.vault_path.display());
    let options = SqliteConnectOptions::from_str(&db_path)?
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .pragma("mmap_size", "30000000000")
        .pragma("temp_store", "MEMORY")
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(50)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    info!(Module::Database, "Applied all pending SQLite migrations");

    Ok(pool)
}
