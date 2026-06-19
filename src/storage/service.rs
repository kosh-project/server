use bytes::Bytes;
use futures::{Stream, StreamExt};
use std::error::Error as StdErr;
use std::io::{Error as IoErr, ErrorKind as IoErrKind};
use tokio::fs::{remove_file, rename};
use tokio::{fs::File, io::AsyncWriteExt};

use crate::log;
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
        let (temp_path, final_path) = self.validate(&file_name)?;

        if let Err(x) = self.try_commit(f_stream, &temp_path, &final_path).await {
            remove_file(temp_path).await?;
            return Err(x);
        }

        log!("STORAGE", "committed: {file_name}");

        Ok(())
    }

    /// Validates
    fn validate<T>(&self, file: &T) -> Result<(PathBuf, PathBuf)>
    where
        T: AsRef<str>,
    {
        let file_name = file.as_ref();
        if file_name.is_empty() || file_name.contains('/') || file_name.contains('\\') {
            return Err(InvalidFileName);
        }

        let final_path = self.vault_path.join(file_name);
        let temp_path = self.vault_path.join(format!("{file_name}.tmp"));

        if final_path.exists() {
            return Err(FileAlreadyExists(file_name.to_string()));
        }

        Ok((temp_path, final_path))
    }

    async fn try_commit<S, E>(
        &self,
        mut f_stream: S,
        temp_path: &Path,
        final_path: &Path,
    ) -> Result<()>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let mut file = File::create(&temp_path).await.map_err(|e| CreateTempFile {
            path: temp_path.into(),
            source: e,
        })?;

        while let Some(chunk) = f_stream.next().await {
            let chunk = chunk.map_err(|e| IoErr::new(IoErrKind::Other, e))?;

            file.write_all(&chunk)
                .await
                .map_err(|e| WriteChunkFailure {
                    path: temp_path.into(),
                    source: e,
                })?;
        }

        rename(&temp_path, &final_path)
            .await
            .map_err(|e| RenameError {
                path: final_path.into(),
                source: e,
            })?;
        Ok(())
    }
}
