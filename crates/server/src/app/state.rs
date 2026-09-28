use std::path::Path;

use moka::future::Cache;
use sqlx::SqlitePool;

use crate::{
    auth::Secret,
    model::session::TokenHash,
    storage::{self, ledger},
};

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
    /// The handle to the background ledger actor that manages per-user delta sync files.
    ///
    /// Cloning this is cheap — the `Handle` wraps an `mpsc::Sender` backed by an `Arc`.
    pub ledger: ledger::Handle,
    /// In-memory session cache. Checked before every database lookup in `auth_guard`
    /// to avoid hitting the disk on every authenticated request.
    pub session_cache: Cache<TokenHash, UserId>,
    /// The persistent session signing key (`K_server`).
    ///
    /// Loaded from or generated into `vault/server.secret` at startup. Survives
    /// reboots so that active client sessions remain valid across power cycles.
    /// Used exclusively by [`mac_guard`] and [`Session::create`].
    ///
    /// [`mac_guard`]: crate::api::middleware::mac_guard
    /// [`Session::create`]: crate::model::session::Session::create
    pub secret: Secret,
    /// The ephemeral Hashcash signing key (`K_ephemeral`).
    ///
    /// Generated fresh in RAM on every server boot via [`Secret::random`].
    /// Used exclusively to sign and verify 15-second Hashcash `PoW` challenges.
    /// Because it is never persisted, all pending challenges are automatically
    /// invalidated when the server restarts.
    pub pow_secret: Secret,

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
