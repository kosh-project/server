use bytes::Bytes;
use futures::{Stream, StreamExt};
use std::error::Error as StdErr;
use std::io::{Error as IoErr, ErrorKind as IoErrKind};
use tokio::fs::rename;
use tokio::{fs::File, io::AsyncWriteExt};

use crate::log;
use crate::storage::{Error::*, Result};
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
    pub async fn save<S, E>(&self, file_name: &str, mut f_stream: S) -> Result<()>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        if file_name.is_empty() || file_name.contains('/') || file_name.contains('\\') {
            return Err(InvalidFileName);
        }

        let final_path = self.vault_path.join(file_name);

        let temp_path = self.vault_path.join(format!("{file_name}.tmp"));

        if final_path.exists() {
            return Err(FileAlreadyExists(file_name.to_string()));
        }

        let mut file = File::create(&temp_path).await.map_err(|e| CreateTempFile {
            path: temp_path.clone(),
            source: e,
        })?;

        while let Some(chunk_result) = f_stream.next().await {
            let chunk = chunk_result.map_err(|err| IoErr::new(IoErrKind::Other, err))?;

            file.write_all(&chunk)
                .await
                .map_err(|e| WriteChunkFailure {
                    path: temp_path.clone(),
                    source: e,
                })?;
        }

        rename(&temp_path, &final_path)
            .await
            .map_err(|e| RenameError {
                path: final_path,
                source: e,
            })?;

        log!("STORAGE", "committed: {file_name}");

        Ok(())
    }
}
