use crate::storage::{Error::*, Result, file::Metadata};
use blake3::Hasher;
use bytes::Bytes;
use futures::{Stream, StreamExt};
use std::{
    error::Error as StdErr,
    io::{Error as IoErr, ErrorKind as IoErrKind},
    path::{Path, PathBuf},
};
use tokio::{fs::*, io::AsyncWriteExt};
use uuid::Uuid;

#[derive(Debug)]
pub(crate) struct Transaction {
    temp: PathBuf,
    _uuid: Uuid,
    hasher: Hasher,
}

/// Setter and getters
#[allow(unused)]
impl Transaction {
    pub(crate) fn new<P>(vault_path: P) -> Self
    where
        PathBuf: From<P>,
    {
        let uuid = Uuid::new_v4();
        let vault_path: PathBuf = vault_path.into();
        let mut temp = vault_path.join(uuid.to_string());
        temp.add_extension("tmp");
        Self {
            temp,
            _uuid: uuid,
            hasher: Hasher::new(),
        }
    }

    pub(crate) fn temp_path(&self) -> &Path {
        &self.temp
    }
}

/// Actions
impl Transaction {
    pub async fn commit<S, E>(mut self, f_stream: S) -> Result<Metadata>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        match self.write_and_commit(f_stream).await {
            Ok(metadata) => Ok(metadata),

            // Deletes temporary file if error occurs
            // Future : queue this file for gc, after some period
            // Until this garbage is collected by Gc, upload of same file can be resumed and this garbage can be reused
            Err(e) => {
                // Needless to report if this fails, otherwise main error `e` gets dropped
                let _ = remove_file(self.temp).await;
                return Err(e);
            }
        }
    }

    async fn write_and_commit<S, E>(
        &mut self,
        f_stream: S,
    ) -> Result<Metadata>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let mut file = File::create(&self.temp).await.map_err(|e| {
            CreateTempFile {
                path: self.temp.clone(),
                source: e,
            }
        })?;

        self.process_stream(f_stream, &mut file).await?;

        let target = self
            .temp
            .parent()
            .ok_or(InvalidPath {
                path: self.temp.clone(),
            })?
            .join(self.hasher.finalize().to_string());

        rename(&self.temp, &target).await.map_err(|e| RenameError {
            path: target,
            source: e,
        })?;

        Metadata::try_new(
            &file.metadata().await?,
            self.hasher.finalize(),
        )
    }

    async fn process_stream<S, E>(
        &mut self,
        mut f_stream: S,
        file: &mut File,
    ) -> Result<()>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        while let Some(chunk) = f_stream.next().await {
            let chunk =
                chunk.map_err(|e| IoErr::new(IoErrKind::Other, e))?;

            file.write_all(&chunk).await.map_err(|e| {
                WriteChunkFailure {
                    path: self.temp.clone(),
                    source: e,
                }
            })?;

            self.hasher.update(&chunk);
        }
        Ok(())
    }
}

impl AsRef<Transaction> for Transaction {
    fn as_ref(&self) -> &Transaction {
        self
    }
}

#[cfg(test)]
mod test {
    use blake3::Hasher;
    use bytes::Bytes;
    use std::io::{Error as IoErr, ErrorKind};

    use crate::storage::tests::with_temp_transaction;

    #[tokio::test]
    async fn successful_commit_and_hash() {
        with_temp_transaction(async move |transaction, vault_path| {
            let chunks: Vec<Result<Bytes, IoErr>> = vec![
                Ok(Bytes::from("hello")),
                Ok(Bytes::from(" ")),
                Ok(Bytes::from("world")),
            ];

            let f_stream = futures::stream::iter(chunks);

            let result = transaction.commit(f_stream).await;

            assert!(result.is_ok());

            let metadata = result.as_ref().unwrap();

            let target_path =
                vault_path.join(metadata.hash.to_string());

            let mut hasher = Hasher::new();
            let bytes = tokio::fs::read(target_path).await.unwrap();
            hasher.update(&bytes);

            let expected_hash = hasher.finalize().to_string();

            assert_eq!(expected_hash, metadata.hash.to_string());
        })
        .await
    }

    #[tokio::test]
    async fn aborted_test_cleans_up_garbage() {
        with_temp_transaction(async move |transaction, _vault_path| {
            let temp_path = transaction.temp_path().to_owned();

            let chunks: Vec<Result<Bytes, IoErr>> = vec![
                Ok(Bytes::from("good bytes")),
                Err(IoErr::new(
                    ErrorKind::ConnectionAborted,
                    "Wifi dies, lol",
                )),
            ];

            let f_stream = futures::stream::iter(chunks);

            let result = transaction.commit(f_stream).await;

            assert!(result.is_err());

            assert!(!temp_path.exists());
            // assert!(!target_path.exists());
        })
        .await;
    }
}
