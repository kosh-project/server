use crate::storage::{
    Error::{
        CreateTempFile, InvalidPath, RenameError, WriteChunkFailure,
    },
    Payload, Result,
    file::Metadata,
};
use blake3::Hasher;
use bytes::Bytes;
use fs4::AsyncFileExt;
use futures::{Stream, StreamExt};
use std::{
    error::Error as StdErr,
    io::{Error as IoErr, ErrorKind::UnexpectedEof},
    path::{Path, PathBuf},
};
use tokio::{
    fs::{File, remove_file, rename},
    io::AsyncWriteExt,
};
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

impl Transaction {
    pub async fn commit<S, E>(
        mut self,
        payload: Payload<S, E>,
    ) -> Result<Metadata>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        match self.write_and_commit(payload).await {
            Ok(metadata) => Ok(metadata),

            // Deletes temporary file if error occurs
            // Future : queue this file for gc, after some period
            // Until this garbage is collected by Gc, upload of same file can be resumed and this garbage can be reused
            Err(e) => {
                // Needless to report if this fails, otherwise main error `e` gets dropped
                let _ = remove_file(self.temp).await;

                Err(e)
            }
        }
    }

    async fn write_and_commit<S, E>(
        &mut self,
        payload: Payload<S, E>,
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

        if payload.expected_size > 0 {
            let _ = file.allocate(payload.expected_size).await;
        }

        let bytes_written =
            self.process_stream(payload.stream, &mut file).await?;

        if bytes_written != payload.expected_size {
            return Err(IoErr::new(
                UnexpectedEof,
                "Content-Length doesn't match the bytes streamed",
            )
            .into());
        }

        let target = self
            .temp
            .parent()
            .ok_or_else(|| InvalidPath {
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
    ) -> Result<u64>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let mut bytes_written: u64 = 0;

        #[allow(clippy::arithmetic_side_effects)]
        // Never overflows because max filesize itself is 10 GBs
        while let Some(chunk) = f_stream.next().await {
            let chunk = chunk.map_err(|e| IoErr::other(e))?;

            file.write_all(&chunk).await.map_err(|e| {
                WriteChunkFailure {
                    path: self.temp.clone(),
                    source: e,
                }
            })?;

            bytes_written += u64::try_from(chunk.len())?;

            self.hasher.update(&chunk);
        }
        Ok(bytes_written)
    }
}

impl AsRef<Self> for Transaction {
    fn as_ref(&self) -> &Self {
        self
    }
}

#[cfg(test)]
mod test {
    use blake3::Hasher;
    use bytes::Bytes;
    use std::{
        io::{Error as IoErr, ErrorKind},
        path::PathBuf,
    };

    use crate::storage::{
        Payload, tests::with_temp_transaction, transaction::Transaction,
    };

    #[tokio::test]
    async fn successful_commit_and_hash() {
        with_temp_transaction(async move |transaction, vault_path| {
            let chunks: Vec<Result<Bytes, IoErr>> = vec![
                Ok(Bytes::from("hello")),
                Ok(Bytes::from(" ")),
                Ok(Bytes::from("world")),
            ];

            let payload =
                Payload::new(11 as u64, futures::stream::iter(chunks));

            let result = transaction.commit(payload).await;

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
    async fn zero_byte_stream_creates_empty_file() {
        with_temp_transaction(async move |transaction, vault_path| {
            let chunks: Vec<Result<Bytes, IoErr>> = Vec::new();

            let payload =
                Payload::new(0 as u64, futures::stream::iter(chunks));

            let result = transaction.commit(payload).await;

            // Test: Should succeed w/o panic
            assert!(result.is_ok());

            let metadata = result.unwrap();

            assert_eq!(metadata.size, 0);

            let target_path =
                vault_path.join(metadata.hash.to_string());

            // Test: There File should be present, even though its empty
            assert!(target_path.exists());

            let expected_hash = Hasher::new().finalize().to_string();

            // Test: Hashes match
            assert_eq!(metadata.hash.to_string(), expected_hash)
        })
        .await;
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

            let payload =
                Payload::new(20 as u64, futures::stream::iter(chunks));

            let result = transaction.commit(payload).await;

            assert!(result.is_err());

            assert!(!temp_path.exists());
            // assert!(!target_path.exists());
        })
        .await;
    }

    #[tokio::test]
    async fn transaction_fails_if_vault_missing() {
        let vault =
            PathBuf::from("/tmp/path/that/possibly/doesnt/exist/lol");
        let transaction = Transaction::new(vault);

        let chunks: Vec<Result<Bytes, IoErr>> =
            vec![Ok(Bytes::from("data_data"))];
        let f_stream = futures::stream::iter(chunks);

        let payload = Payload::new(9u64, f_stream);

        let result = transaction.commit(payload).await;

        // Test: No problem parsing the data
        assert!(result.is_err());

        use crate::storage::Error::CreateTempFile;

        // Test: Yields CreateTempFile Error, 'cause vault directory was missing
        assert!(
            matches!(result, Err(CreateTempFile { .. })),
            "Expected Err(CreateTempFile)"
        );
    }

    #[tokio::test]
    async fn hardcoded_hash_correctness() {
        with_temp_transaction(async move |transaction, _vault_path| {
            let payload : Vec<Result<Bytes, IoErr>> = vec![Ok(Bytes::from("hello world"))];
            let f_stream = futures::stream::iter(payload);

            let payload = Payload::new(11 as u64, f_stream);

            let metadata = transaction.commit(payload).await.unwrap();

            // Pre-calculated Blake3 hash of "hello world"
            let expected_hash = "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24";

            // Test: committed payload generates same hash as expected_hash
            assert_eq!(metadata.hash.to_string(), expected_hash);
        }).await;
    }

    #[tokio::test]
    async fn mismatch_content_fails_plus_cleans_up() {
        with_temp_transaction(async move |transaction, _| {
            let temp_path = transaction.temp_path().to_owned();

            let chunks: Vec<Result<Bytes, IoErr>> =
                vec![Ok(Bytes::from("Halo there"))];

            let payload =
                Payload::new(67u64, futures::stream::iter(chunks));

            let result = transaction.commit(payload).await;

            // Test: Unexpected EOF causes failure
            assert!(result.is_err());

            // Test: Cleanup is expected on failure
            assert!(!temp_path.exists());
        })
        .await;
    }
}
