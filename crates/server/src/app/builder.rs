use std::{path::PathBuf, time::Duration};

use moka::future::Cache;
use sqlx::SqlitePool;
use tokio::sync::mpsc::Sender;

use crate::{
    app::{State as AppState, state::UserId},
    model::session::TokenHash,
    storage,
};

/// A builder for constructing [`AppState`].
///
/// This is the intended way to create an `AppState` both in `main.rs` and in tests.
/// Using a builder makes it easy to inject different databases and vault paths
/// in test environments without modifying the production code path.
///
/// # Example
///
/// ```rust,no_run
/// use webdav_server::app::AppStateBuilder;
///
/// # let pool = todo!(); // SqlitePool — provided at startup
/// let state = AppStateBuilder::new()
///     .vault_path("./vault")
///     .db(pool)
///     .build();
/// ```
///
/// [`AppState`]: crate::app::State
pub struct AppStateBuilder {
    vault_path: Option<PathBuf>,
    db: Option<SqlitePool>,
    session_cache: Option<Cache<TokenHash, UserId>>,
}

impl AppStateBuilder {
    /// Creates a new builder with no fields configured.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            vault_path: None,
            db: None,
            session_cache: None,
        }
    }

    /// Sets the path to the vault directory where blobs are stored.
    ///
    /// This field is required. Calling `build()` without setting it will panic.
    #[must_use]
    pub fn vault_path<P>(mut self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.vault_path = Some(path.into());
        self
    }

    /// Sets the `SqlitePool` for all database operations.
    ///
    /// This field is required. Calling `build()` without setting it will panic.
    #[must_use]
    pub fn db(mut self, pool: SqlitePool) -> Self {
        self.db = Some(pool);
        self
    }

    /// Optionally provides a pre-configured `moka` session cache.
    ///
    /// If not set, `build()` will create a default cache with a 10-minute
    /// Time-To-Idle (TTI) policy and a maximum capacity of 10,000 entries.
    /// This default is suitable for most deployments.
    #[must_use]
    pub fn session_cache(
        mut self,
        session_cache: Cache<TokenHash, UserId>,
    ) -> Self {
        self.session_cache = Some(session_cache);
        self
    }

    /// Consumes the builder and returns the fully configured `AppState`.
    ///
    /// If `session_cache` was not set, a default `moka` cache is created with
    /// a 10-minute TTI and 10,000-entry capacity.
    ///
    /// # Panics
    /// Panics if `vault_path` or `db` were not configured before calling `build`.
    #[allow(clippy::expect_used)]
    #[must_use]
    pub fn build(self) -> AppState {
        AppState {
            storage: storage::Service::new(
                self.vault_path
                    .expect("FATAL: vault_path is required!"),
            ),
            db: self.db.expect("FATAL: database pool is required!"),
            session_cache: self.session_cache.unwrap_or_else(|| {
                Cache::builder()
                    .time_to_idle(Duration::from_mins(10))
                    .max_capacity(10_000)
                    .build()
            }),
        }
    }
}

impl Default for AppStateBuilder {
    fn default() -> Self {
        Self::new()
    }
}
