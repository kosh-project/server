use crate::storage::{Error, Result};
use std::path::PathBuf;

pub struct Service {
    vault_path: PathBuf,
}

impl Service {
    pub async fn commit(&self) -> Result<()> {
        todo!()
    }
}
