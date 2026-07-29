use bytes::Bytes;
use futures::Stream;
use std::error::Error as StdErr;

use crate::log;
use crate::storage::file::Metadata;
use crate::storage::transaction::Transaction;
use crate::storage::{Error::*, Payload, Result};
use std::path::PathBuf;

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
    pub async fn try_save<S, E>(
        &self,
        file_name: &str,
        payload: Payload<S, E>,
    ) -> Result<Metadata>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let transaction = self.begin_transaction(&file_name)?;

        let file_metadata = transaction.commit(payload).await?;

        log!("STORAGE", "committed: {file_name}");

        Ok(file_metadata)
    }

    /// Validates
    fn begin_transaction<T>(&self, file: &T) -> Result<Transaction>
    where
        T: AsRef<str>,
    {
        let file_name = file.as_ref();
        if file_name.is_empty()
            || file_name.contains('/')
            || file_name.contains('\\')
        {
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
    use core::result::Result;
    use std::{io::Error as IoErr, result};

    use tokio::fs::File;

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

            let result =
                service.begin_transaction(&"../../../../etc/passwd");
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
    async fn concurrent_write_collisions_dont_panic() {
        with_temp_service(|service| async move {
            let service_a = service.clone();
            let service_b = service.clone();

            let task_a = tokio::spawn(async move {
                let chunks: Vec<Result<Bytes, IoErr>> =
                    vec![Ok(Bytes::from("some_data"))];
                let stream = futures::stream::iter(chunks);

                service_a
                    .try_save(
                        "dev1_upload.rs",
                        Payload::new(9u64, stream),
                    )
                    .await
            });

            let task_b = tokio::spawn(async move {
                let payload: Vec<Result<Bytes, IoErr>> =
                    vec![Ok(Bytes::from("some_data"))];
                let stream = futures::stream::iter(payload);

                service_b
                    .try_save(
                        "some_other_file.rs",
                        Payload::new(9u64, stream),
                    )
                    .await
            });

            let (result_a, result_b) = tokio::join!(task_a, task_b);

            // Test : Writing to same file doesn't fail
            let metadata_a = result_a.unwrap().expect("task_a failed");
            let metadata_b = result_b.unwrap().expect("task_b failed");

            // Test: Both files wrote exact same data
            assert_eq!(
                metadata_a.hash.to_string(),
                metadata_b.hash.to_string()
            );

            let expected_path =
                service.vault_path.join(metadata_a.hash.to_string());
            // Test: Expected path exists
            assert!(expected_path.exists());
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
