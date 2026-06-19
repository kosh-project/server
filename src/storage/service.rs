use bytes::Bytes;
use futures::{Stream, StreamExt};
use std::error::Error as StdErr;
use std::io::{Error as IoErr, ErrorKind as IoErrKind};
use tokio::fs::{remove_file, rename};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::log;
use crate::storage::transaction::{self, Transaction};
use crate::storage::{Error::*, Result};
use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub struct Service {
    pub(crate) vault_path: PathBuf,
}

impl Service {
    pub fn new<P>(vault_path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            vault_path: vault_path.into(),
        }
    }

    // Needs more refactoring
    pub async fn try_save<S, E>(&self, file_name: &str, f_stream: S) -> Result<()>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let transaction = self.begin_transaction(&file_name)?;

        // Deletes temporary file if error occurs
        // Future : queue this file for gc, after some period
        // Until this garbage is collected by Gc, upload of same file can be resumed and this garbage can be reused
        let hash = transaction.commit(f_stream).await?;

        log!("STORAGE", "committed: {file_name}");

        Ok(())
    }

    /// Validates
    fn begin_transaction<T>(&self, file: &T) -> Result<Transaction>
    where
        T: AsRef<str>,
    {
        let file_name = file.as_ref();
        if file_name.is_empty() || file_name.contains('/') || file_name.contains('\\') {
            return Err(InvalidFileName);
        }

        let target_path = self.vault_path.join(file_name);
        let temp_path = self.vault_path.join(format!("{file_name}.tmp"));

        if target_path.exists() {
            return Err(FileAlreadyExists(file_name.to_string()));
        }

        Ok(Transaction::new(temp_path, target_path))
    }
}
