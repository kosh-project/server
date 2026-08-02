use tokio::io;

use crate::error::internal;

/// Errors that may occur, when handling API Requests
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Malformed Multipart found")]
    MalformedMultipart,
    #[error("Bad Request")]
    BadRequest(String),
    #[error("Stream Read Error : ")]
    StreamReadError,
    #[error("Missing Field")]
    MissingField,
    #[error("IO Error : {}", .0)]
    IoError(#[from] io::Error),
    #[error("Unauthorized : {}", .0)]
    Unauthorized(String),
    #[error("Internal Server Error : {}", .0)]
    Internal(#[from] internal::Error),
    #[error("Not Found")]
    NotFound(String)
}

pub type Result<T> = core::result::Result<T, Error>;
