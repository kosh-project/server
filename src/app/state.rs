use std::path::Path;

use sqlx::SqlitePool;

use crate::storage;

#[derive(Clone)]
pub struct State {
    pub storage: storage::Service,
    pub db: SqlitePool,
}

impl State {
    // pub fn vault_path(&self) -> &Path {
    //     todo!()
    // }

    pub fn vault_path(&self) -> &Path {
        &self.storage.vault_path
    }
}
