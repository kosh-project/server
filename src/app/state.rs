use std::path::Path;

use crate::storage;

#[derive(Clone, Default)]
pub struct State {
    pub storage: storage::Service,
}

impl State {
    // pub fn vault_path(&self) -> &Path {
    //     todo!()
    // }

    pub fn vault_path(&self) -> &Path {
        &self.storage.vault_path
    }
}
