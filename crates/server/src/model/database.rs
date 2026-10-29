use std::str::FromStr;

use kosh_core::{config::Config, logger::Module};
use sqlx::{
    SqlitePool,
    sqlite::{
        SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};
use crate::{error::boot::Result, info};

/// Initializes the SQLite metadata database pool and applies pending migrations.
///
/// # Errors
///
/// Returns [`crate::error::boot::Error::Database`] or [`crate::error::boot::Error::Migration`] if connecting to SQLite or running migrations fails.
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
