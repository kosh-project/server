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
    pub async fn try_save<S, E>(&self, file_name: &str, f_stream: S) -> Result<String>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let transaction = self.begin_transaction(&file_name)?;

        let hash = transaction.commit(f_stream).await?;

        log!("STORAGE", "committed: {file_name}");

        Ok(hash)
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

        if target_path.exists() {
            return Err(FileAlreadyExists(file_name.to_string()));
        }

        Ok(Transaction::new(&self.vault_path))
    }
}

#[cfg(test)]
mod tests {
    use serde::ser;

    use super::*;

    use crate::storage::tests::with_temp_service;

    #[tokio::test]
    async fn reject_invalid_filename() -> crate::storage::Result<()> {
        with_temp_service(|service| async move {
            // Reject for any occurrence of forward slash
            let result = service.begin_transaction(&"o///reo/hiuh//i");
            assert!(result.is_err());
            assert!(matches!(result, Err(InvalidFileName)));

            let result = service.begin_transaction(&"");
            assert!(result.is_err());
            assert!(matches!(result, Err(InvalidFileName)));

            let result = service.begin_transaction(&"../../../../etc/passwd");
            assert!(result.is_err());
            assert!(matches!(result, Err(InvalidFileName)))
        })
        .await;

        Ok(())
    }

    #[tokio::test]
    async fn validation_rejects_existing_file() {
        with_temp_service(async move |service| {
            let file_name = "etc.passwd";
            let target_path = service.vault_path.join(file_name);

            let _ = File::create(target_path).await.unwrap();

            let result = service.begin_transaction(&file_name);

            assert!(result.is_err());
            assert!(matches!(result, Err(FileAlreadyExists(_))));
        })
        .await;
    }

    #[tokio::test]
    async fn validation_success() {
        with_temp_service(async move |service| {
            // Valid name rules
            let result = service.begin_transaction(&"oreo.tmp.jks");
            assert!(result.is_ok());
            assert!(matches!(result, Ok(_)));
        })
        .await
    }
}
