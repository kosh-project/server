use std::path::{Path, PathBuf};

#[derive(Clone, Default)]
pub struct AppState {
    pub(crate) vault_path: PathBuf,
}

impl AppState {
    pub fn vault_path(&self) -> &Path {
        self.vault_path.as_ref()
    }
}