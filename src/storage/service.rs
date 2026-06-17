use axum::extract::multipart::Field;
use bytes::Bytes;
use futures::{Stream, StreamExt};
use tokio::{fs::File, io::AsyncWriteExt};
use std::error::Error as StdErr;
use std::result::Result as StdResult;
use std::io::{Error as IoErr, ErrorKind as IoErrKind};

use crate::log;
use crate::storage::{Error, Result};
use std::{
    path::{Path, PathBuf},
    process::Output,
};

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
        if file_name.is_empty() 
        || file_name.contains('/')
        || file_name.contains('\\') {
            return Err(Error::InvalidFileName)
        }

        log!("PATH_CHECK", "Checked path, its cool");

        let path = self.vault_path.join(file_name);

        if path.exists() {
            eprintln!("{}", path.to_str().unwrap());
            return Err(Error::FileAlreadyExists(file_name.to_string()))
        }

        log!("PATH_EXISTS?", "Path doesnt exist cool");

        let mut file = File::create(&path).await
        .map_err(|e| Error::CreateTempFile { path : path.clone(), source: e })?;

        log!("FILE_CREATED", "File created here, now writing ");
        while let Some(chunk_result) = f_stream.next().await {
            let chunk = chunk_result.map_err(|err| 
                IoErr::new(IoErrKind::Other, err)
            )?;

            file.write_all(&chunk).await.map_err(|e| 
                Error::WriteChunkFailure { path : path.clone(), source: e }
            )?;
        }

        Ok(())
    }
}
