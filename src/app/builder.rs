use std::path::PathBuf;

use sqlx::SqlitePool;

use crate::{app::State as AppState, storage};

pub struct AppStateBuilder {
    vault_path: Option<PathBuf>,
    db: Option<SqlitePool>,
}

impl AppStateBuilder {
    pub fn new() -> Self {
        Self {
            vault_path: None,
            db: None,
        }
    }

    pub fn vault_path<P>(mut self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.vault_path = Some(path.into());
        self
    }

    pub fn db(mut self, pool: SqlitePool) -> Self {
        self.db = Some(pool);
        self
    }

    pub fn build(self) -> AppState {
        AppState {
            storage: storage::Service::new(
                self.vault_path.expect("FATAL: vault_path is required!"),
            ),
            db: self.db.expect("FATAL: database pool is required!"),
        }
    }
}
