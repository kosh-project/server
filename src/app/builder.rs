use std::{path::PathBuf, time::Duration};

use moka::future::Cache;
use sqlx::SqlitePool;

use crate::{
    app::{State as AppState, state::UserId},
    model::session::TokenHash,
    storage,
};

pub struct AppStateBuilder {
    vault_path: Option<PathBuf>,
    db: Option<SqlitePool>,
    session_cache: Option<Cache<TokenHash, UserId>>,
}

impl AppStateBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            vault_path: None,
            db: None,
            session_cache: None,
        }
    }

    #[must_use]
    pub fn vault_path<P>(mut self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.vault_path = Some(path.into());
        self
    }

    #[must_use]
    pub fn db(mut self, pool: SqlitePool) -> Self {
        self.db = Some(pool);
        self
    }

    #[must_use]
    pub fn session_cache(
        mut self,
        session_cache: Cache<TokenHash, UserId>,
    ) -> Self {
        self.session_cache = Some(session_cache);
        self
    }

    /// Consumes the builder and returns the constructed `AppState`.
    ///
    /// # Panics
    /// This function will panic if `vault_path` or `db` have not been configured
    /// prior to calling `build`.
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
