use std::path::Path;

use moka::future::Cache;
use sqlx::SqlitePool;

use crate::{model::session::TokenHash, storage};

pub type UserId = i64;

/// The shared application state, injected into every route handler by Axum.
///
/// `State` is cloned cheaply on each request — all fields are either reference-counted
/// (`SqlitePool`, `Cache`) or backed by an `Arc` internally, so cloning is just
/// incrementing a reference count.
///
/// Access it in handlers via `State(state): State<AppState>`.
#[derive(Clone)]
pub struct State {
    /// The storage service managing the on-disk CAS vault.
    pub storage: storage::Service,
    /// The `SQLite` connection pool for all database queries.
    pub db: SqlitePool,
    /// In-memory session cache. Checked before every database lookup in `auth_guard`
    /// to avoid hitting the disk on every authenticated request.
    pub session_cache: Cache<TokenHash, UserId>,
}

impl State {
    /// Returns a reference to the vault directory path.
    ///
    /// This is a convenience accessor that delegates to the storage service.
    #[must_use]
    pub fn vault_path(&self) -> &Path {
        &self.storage.vault_path
    }
}
