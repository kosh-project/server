use tokio::io;

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
}

pub type Result<T> = core::result::Result<T, Error>;
