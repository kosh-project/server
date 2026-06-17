use std::path::PathBuf;

use crate::app::State as AppState;

#[derive(Default)]
pub struct AppStateBuilder(AppState);

impl AppStateBuilder {
    pub fn new() -> Self {
        Self(AppState::default())
    }

    pub fn vault_path<P>(mut self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.0.storage.vault_path = path.into();
        self
    }

    pub fn build(self) -> AppState {
        self.0
    }
}
