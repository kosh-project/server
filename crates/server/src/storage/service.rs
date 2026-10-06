use bytes::Bytes;
use futures::Stream;
use kosh_core::logger::Module;
use std::error::Error as StdErr;
use std::io::ErrorKind::{self};
use tokio::fs::{self, File};

use crate::info;
use crate::storage::file::Metadata;
use crate::storage::transaction::Transaction;
use crate::storage::{
    Error::{InvalidFileName, NotFound},
    Payload, Result,
};
use std::path::PathBuf;

#[derive(Default, Clone)]
/// The high-level interface to the storage vault.
///
/// `Service` is the entry point for all filesystem operations in this application.
/// It owns the path to the vault directory and exposes a small, focused API for
/// saving, retrieving, and deleting blobs.
///
/// Internally, writes go through an internal transaction which guarantees atomicity:
/// a failed write can never leave a corrupted or partial file in the vault.
pub struct Service {
    pub(crate) vault_path: PathBuf,
}

impl Service {
    /// Creates a new `Service` rooted at the given vault directory.
    ///
    /// The path is not validated here — if the directory does not exist when
    /// a transaction is attempted, a [`crate::storage::Error::VaultNotFound`] will be
    /// returned at that point.
    pub fn new<P>(vault_path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            vault_path: vault_path.into(),
        }
    }

    /// Streams a payload to disk and commits it atomically to the vault.
    ///
    /// This is the main entry point for all uploads. Internally it creates an
    /// internal transaction, streams all bytes to a temporary staging file, then
    /// performs an atomic `rename(2)` to the final CAS path (the BLAKE3 hash
    /// of the content). Returns the file [`Metadata`] on success.
    ///
    /// If anything fails mid-stream, the temporary file is cleaned up before
    /// the error is returned.
    ///
    /// # Errors
    /// - Returns [`crate::storage::Error::InvalidFileName`] if `file_name` contains `/`, `\`, or is empty.
    /// - Returns [`crate::storage::Error::FileAlreadyExists`] if a file with that name already exists in the vault.
    /// - Returns [`crate::storage::Error`] if a disk I/O failure occurs during streaming or the atomic rename.
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

        info!(
            Module::Storage,
            "Committed CAS blob for {file_name} to disk"
        );
        Ok(file_metadata)
    }

    /// Validates the filename and creates a new internal transaction ready for streaming.
    ///
    /// This is an internal helper used by [`Service::try_save`]. It enforces the
    /// filename rules (no path separators, no empty names) before any disk I/O
    /// is attempted.
    pub(crate) fn begin_transaction<T>(&self, file: &T) -> Result<Transaction>
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

        Ok(Transaction::new(&self.vault_path))
    }
}

impl Service {
    /// Deletes the blob at the given hash string from the vault.
    ///
    /// This is intentionally idempotent: if the file does not exist, `Ok(())` is
    /// returned without error. This makes it safe to call even when the physical
    /// file has already been removed or was never committed.
    ///
    /// # Errors
    /// - Returns [`crate::storage::Error::Internal`] if the filesystem returns any error other than
    ///   `NotFound` (e.g., permission denied).
    pub async fn delete_blob(&self, hash_str: &str) -> Result<()> {
        let file_path = self.vault_path.join(hash_str);

        match fs::remove_file(&file_path).await {
            Ok(()) => {
                info!(
                    Module::Storage,
                    "Blob '{hash_str}' physically deleted from vault"
                );
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(source) => Err(super::Error::DeleteBlob {
                path: file_path,
                source,
            }),
        }
    }

    /// Opens and returns the blob file at the given hash string for reading.
    ///
    /// The returned [`tokio::fs::File`] handle can be wrapped in a [`ReaderStream`]
    /// for direct streaming to an HTTP response body without loading the file into memory.
    ///
    /// # Errors
    /// - Returns [`crate::storage::Error::NotFound`] if no blob with that hash exists in the vault.
    ///
    /// [`ReaderStream`]: tokio_util::io::ReaderStream
    pub async fn get_blob(&self, hash_str: &str) -> Result<File> {
        let file_path = self.vault_path.join(hash_str);

        File::open(file_path).await.map_err(|_| NotFound)
    }
}
