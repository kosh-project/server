use std::{num::TryFromIntError, path::PathBuf, time::SystemTimeError};

use axum::response::IntoResponse;
use hyper::StatusCode;
use tokio::io;

use crate::{
    error::internal,
    logger::{self, Loggable},
    storage, wrap_internal_err,
};

/// Errors that can occur in the storage layer.
///
/// These are filesystem and transaction-level failures. Most are internal
/// and should never be exposed to the client in detail. The `IntoResponse`
/// implementation handles sanitization automatically.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// The vault directory does not exist on disk.
    /// This is a fatal misconfiguration and means the server was started
    /// with an invalid `vault_path`.
    #[error("Root storage doesn't exist")]
    VaultNotFound,

    /// The provided filename contained illegal characters (`/`, `\`, or was empty).
    /// This protects against path traversal attacks.
    #[error("Invalid File Name")]
    InvalidFileName,

    /// A file with this name already exists at the target path.
    /// This is returned when a duplicate upload is attempted for the exact same filename.
    #[error("File Already Exists : {}", .0)]
    FileAlreadyExists(String),

    /// The server failed to create the temporary staging file before streaming begins.
    #[error("Couldn't create temporary file at {path}")]
    CreateTempFile {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// A write to disk failed mid-stream while appending a chunk to the temporary file.
    #[error("Writing chunk to disk failed, file : {path}")]
    WriteChunkFailure {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// Reading the next chunk from the incoming network stream failed.
    #[error("Couldn't gather next chunk of data : {}", .0)]
    StreamReadError(#[from] io::Error),

    /// The atomic rename from the temporary staging file to the final CAS path failed.
    #[error("Failed to rename file : {path}")]
    RenameError {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// The constructed path pointed outside the vault directory.
    #[error("Invalid Path : {path}")]
    InvalidPath { path: PathBuf },

    /// A low-level internal error, typically from integer or time conversions.
    #[error("Internal Error : {}", .0)]
    Internal(#[from] internal::Error),

    /// The requested blob was not found in the vault.
    #[error("Blob Not found")]
    NotFound,
}

pub type Result<T> = core::result::Result<T, storage::Error>;

wrap_internal_err! { TryFromIntError, SystemTimeError => Error::Internal }

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        use Error::{
            CreateTempFile, FileAlreadyExists, Internal, InvalidFileName,
            InvalidPath, NotFound, RenameError, StreamReadError, VaultNotFound,
            WriteChunkFailure,
        };

        match self {
            // Client-facing errors: safe to expose details.
            InvalidFileName | InvalidPath { .. } => {
                (StatusCode::BAD_REQUEST, "Invalid file path or name".into())
            }
            FileAlreadyExists(msg) => (StatusCode::CONFLICT, msg),
            NotFound => (StatusCode::NOT_FOUND, "Blob not found".into()),

            // Internal errors: hide details from the client.
            VaultNotFound
            | CreateTempFile { .. }
            | WriteChunkFailure { .. }
            | StreamReadError(_)
            | RenameError { .. }
            | Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".into(),
            ),
        }
        .into_response()
    }
}

impl Loggable for Error {
    fn log_level(&self) -> crate::logger::Level {
        use logger::Level;
        match self {
            Self::VaultNotFound => Level::Fatal,
            _ => Level::Error,
        }
    }

    fn log_module(&self) -> logger::Module {
        logger::Module::Storage
    }
}
