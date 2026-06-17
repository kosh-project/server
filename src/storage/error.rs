use std::path::PathBuf;

use tokio::io;

use crate::storage;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Root storage doesn't exist")]
    VaultNotFound,
    #[error("Invalid File Name")]
    InvalidFileName,
    #[error("File Already Exists : {}", .0)]
    FileAlreadyExists(String),
    #[error("Couldn't create temporary file at {path}")]
    CreateTempFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Writing chunk to disk failed, file : {path}")]
    WriteChunkFailure {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Couldn't gather next chunk of data : {}", .0)]
    StreamReadError(#[from] io::Error),
    // #[error("Uploaded content generates different digest from what is proposed")]
    // Hash Mismatch
    // #[error("Metadata Updation Failed, i'll see it")]
    // MetadataUpdation
}

pub type Result<T> = core::result::Result<T, storage::Error>;
