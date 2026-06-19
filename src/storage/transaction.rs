use crate::{
    encode,
    storage::{Error::*, Result},
};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use sha2::{Digest, Sha256};
use std::{
    error::Error as StdErr,
    io::{Error as IoErr, ErrorKind as IoErrKind},
    path::{Path, PathBuf},
};
use tokio::{fs::*, io::AsyncWriteExt};

pub(crate) struct Transaction {
    temp: PathBuf,
    target: PathBuf,
    hasher: Sha256,
}

/// Setter and getters
#[allow(unused)]
impl Transaction {
    pub(crate) fn new<P, Pa>(temp_path: P, target_path: Pa) -> Self
    where
        PathBuf: From<P> + From<Pa>,
    {
        Self {
            temp: temp_path.into(),
            target: target_path.into(),
            hasher: Sha256::new(),
        }
    }

    pub(crate) fn temp_path(&self) -> &Path {
        self.temp.as_ref()
    }

    pub(crate) fn target_path(&self) -> &Path {
        self.target.as_ref()
    }
}

/// Actions
impl Transaction {
    pub async fn commit<S, E>(mut self, f_stream: S) -> Result<String>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        match self.write_and_commit(f_stream).await {
            Ok(_) => Ok(encode(self.hasher.finalize().iter())),
            Err(e) => {
                let _ = remove_file(self.temp).await;
                return Err(e);
            }
        }
    }

    async fn write_and_commit<S, E>(&mut self, f_stream: S) -> Result<()>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        let mut file = File::create(&self.temp).await.map_err(|e| CreateTempFile {
            path: self.temp.clone(),
            source: e,
        })?;

        self.process_stream(f_stream, &mut file).await?;

        rename(&self.temp, &self.target)
            .await
            .map_err(|e| RenameError {
                path: self.target.clone(),
                source: e,
            })?;

        Ok(())
    }

    async fn process_stream<S, E>(&mut self, mut f_stream: S, file: &mut File) -> Result<()>
    where
        S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
        E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        while let Some(chunk) = f_stream.next().await {
            let chunk = chunk.map_err(|e| IoErr::new(IoErrKind::Other, e))?;

            file.write_all(&chunk)
                .await
                .map_err(|e| WriteChunkFailure {
                    path: self.temp.clone(),
                    source: e,
                })?;

            self.hasher.update(chunk);
        }
        Ok(())
    }
}

impl AsRef<Transaction> for Transaction {
    fn as_ref(&self) -> &Transaction {
        self
    }
}
