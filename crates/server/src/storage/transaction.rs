use crate::{
    storage::{
        Error::{CreateTempFile, InvalidPath, RenameError, WriteChunkFailure},
        Payload, Result,
        file::Metadata,
    },
    warn,
};
use blake3::Hasher;
use bytes::Bytes;
use fs4::AsyncFileExt;
use futures::{Stream, StreamExt};
use kosh_core::logger::Module;
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
/// A single write transaction for committing a blob to the vault.
///
/// A `Transaction` is created by [`crate::storage::service::Service::try_save`] and
/// represents the lifecycle of one upload from start to finish. It manages two files:
///
/// 1. A temporary staging file at `<vault>/<uuid>.tmp`, where bytes are streamed.
/// 2. The final blob file at `<vault>/<blake3_hash>`, which is created via an atomic `rename(2)`.
///
/// If the transaction fails at any point, the `.tmp` file is deleted before the error
/// is returned. This ensures the vault always contains only successfully committed blobs.
pub(crate) struct Transaction {
    temp: PathBuf,
    _uuid: Uuid,
    hasher: Hasher,
}

/// Constructors and accessors.
#[allow(unused)]
impl Transaction {
    /// Creates a new transaction rooted in the given vault directory.
    ///
    /// A new UUID is generated on each call to ensure the staging file path
    /// is unique, which prevents collisions between concurrent uploads.
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

    /// Returns the path of the temporary staging file.
    ///
    /// Primarily exposed for tests that want to assert cleanup happened.
    pub(crate) fn temp_path(&self) -> &Path {
        &self.temp
    }
}

impl Transaction {
    /// Streams the payload to disk and commits it atomically.
    ///
    /// This is the public entry point. It delegates to `write_and_commit` and
    /// guarantees that the temporary staging file is deleted if anything goes wrong.
    ///
    /// # Errors
    /// See [`Transaction::write_and_commit`] for the full list of failure modes.
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

            Err(e) => {
                // Clean up the staging file before propagating the error.
                // If the cleanup itself fails, we silently ignore it so the
                // original error `e` is not lost.
                //
                // Future: instead of deleting, queue for GC so interrupted
                // uploads can be resumed (tus-style resumable uploads).
                let _ = remove_file(self.temp).await;

                warn!(
                    Module::Storage,
                    "Transaction rolled back and temp file deleted due to error: {e}"
                );

                Err(e)
            }
        }
    }

    /// The internal implementation of the write pipeline.
    ///
    /// Steps:
    /// 1. Create the `.tmp` staging file.
    /// 2. Pre-allocate the expected number of bytes on disk via `fallocate`
    ///    (skipped for zero-byte streams). This reduces fragmentation on
    ///    spinning drives and SD cards.
    /// 3. Stream all chunks to disk, hashing each one incrementally with BLAKE3.
    /// 4. Validate that the bytes written match `expected_size` (EOF check).
    /// 5. Atomically rename the `.tmp` file to `<vault>/<blake3_hash>`.
    ///
    /// # Errors
    /// - [`Error::CreateTempFile`] if the staging file cannot be created.
    /// - [`Error::StreamReadError`] if a chunk cannot be read from the network stream.
    /// - [`Error::WriteChunkFailure`] if a chunk cannot be written to disk.
    /// - [`Error::StreamReadError`] if `bytes_written != expected_size` at EOF.
    /// - [`Error::InvalidPath`] if the staging file has no parent directory.
    /// - [`Error::RenameError`] if the atomic rename fails.
    ///
    /// [`Error::CreateTempFile`]: crate::storage::Error::CreateTempFile
    /// [`Error::StreamReadError`]: crate::storage::Error::StreamReadError
    /// [`Error::WriteChunkFailure`]: crate::storage::Error::WriteChunkFailure
    /// [`Error::InvalidPath`]: crate::storage::Error::InvalidPath
    /// [`Error::RenameError`]: crate::storage::Error::RenameError
    async fn write_and_commit<S, E>(
        &mut self,
        payload: Payload<S, E>,
    ) -> Result<Metadata>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let mut file =
            File::create(&self.temp).await.map_err(|e| CreateTempFile {
                path: self.temp.clone(),
                source: e,
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

        Metadata::try_new(&file.metadata().await?, self.hasher.finalize())
    }

    /// Reads all chunks from the stream, writes them to disk, and returns the total bytes written.
    ///
    /// Each chunk is also fed to the BLAKE3 hasher incrementally, so there is no
    /// need to re-read the file after streaming to compute the hash.
    ///
    /// # Errors
    /// - [`Error::StreamReadError`] if the network stream yields an error on a chunk.
    /// - [`Error::WriteChunkFailure`] if writing a chunk to disk fails.
    ///
    /// [`Error::StreamReadError`]: crate::storage::Error::StreamReadError
    /// [`Error::WriteChunkFailure`]: crate::storage::Error::WriteChunkFailure
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

        // The addition here is safe: the upload handler rejects payloads larger
        // than 10 GB, so `bytes_written` can never overflow a u64.
        #[allow(clippy::arithmetic_side_effects)]
        while let Some(chunk) = f_stream.next().await {
            let chunk = chunk.map_err(|e| IoErr::other(e))?;

            file.write_all(&chunk)
                .await
                .map_err(|e| WriteChunkFailure {
                    path: self.temp.clone(),
                    source: e,
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
