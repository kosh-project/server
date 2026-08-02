use std::{num::TryFromIntError, path::PathBuf, time::SystemTimeError};

use tokio::io;

use crate::{error::internal, storage, wrap_internal_err};

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
    #[error("Failed to rename file : {path}")]
    RenameError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("Invalid Path : {path}")]
    InvalidPath { path: PathBuf },

    #[error("Internal Error : {}", .0)]
    Internal(#[from] internal::Error),

    #[error("Blob Not found")]
    NotFound
}

pub type Result<T> = core::result::Result<T, storage::Error>;


wrap_internal_err! { TryFromIntError, SystemTimeError => Error::Internal }
