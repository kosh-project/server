use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub struct AppState {
    vault_path: PathBuf,
}

impl AppState {
    pub fn vault_path(&self) -> &Path {
        self.vault_path.as_ref()
    }
}

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
        self.0.vault_path = path.into();
        self
    }

    pub fn build(self) -> AppState {
        self.0
    }
}
