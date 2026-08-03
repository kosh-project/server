use std::path::Path;

use moka::future::Cache;
use sqlx::SqlitePool;

use crate::{model::session::TokenHash, storage};

pub type UserId = i64;

#[derive(Clone)]
pub struct State {
    pub storage: storage::Service,
    pub db: SqlitePool,
    pub session_cache: Cache<TokenHash, UserId>,
}

impl State {
    #[must_use]
    pub fn vault_path(&self) -> &Path {
        &self.storage.vault_path
    }
}
