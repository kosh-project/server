use axum::response::IntoResponse;
use hyper::StatusCode;

use crate::logger::{Level, Loggable, Module};

/// Errors that can occur within the delta-CRDT sync ledger subsystem.
///
/// Each variant maps to a specific HTTP status code through the [`IntoResponse`]
/// implementation, ensuring that client-facing errors (invalid file names, bad
/// offsets) produce `4xx` responses while internal failures (IO errors, actor
/// crashes) produce `500 Internal Server Error` without leaking implementation
/// details.
///
/// Severity levels for server-side telemetry are reported through the [`Loggable`]
/// implementation on this type.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// The [`Committer`] actor task terminated while a caller was still holding a
    /// [`Handle`]. This can happen if the actor panicked or the channel was
    /// closed unexpectedly.
    ///
    /// This is a fatal condition that indicates a bug or resource exhaustion.
    /// The request that triggered it will receive a `500 Internal Server Error`.
    ///
    /// [`Committer`]: crate::storage::ledger::committer::Committer
    /// [`Handle`]: crate::storage::ledger::handle::Handle
    #[error("Comitter died before an active sender")]
    CommitterDead,

    /// A segment file exists but its contents are invalid.
    ///
    /// The inner `String` contains a human-readable description of why the
    /// file was rejected, e.g. `"Smaller than 500-bytes header"` or
    /// `"Invalid KOSH Signature"`. This information is logged for debugging
    /// but is never sent to the client.
    ///
    /// The server does not automatically recover from a corrupted segment.
    /// A human operator must inspect the file and decide whether to delete it
    /// and start a new ledger chain.
    #[error("Corrupted segment: {}",.0)]
    CorruptedSegment(String),

    /// The segment filename could not be parsed to extract its numeric ID.
    ///
    /// This should not occur in normal operation because all filenames are
    /// written by the server. It indicates that an unexpected file was placed
    /// in the user's ledger directory.
    #[error(transparent)]
    ParseIntFailure(#[from] std::num::ParseIntError),

    /// A filesystem operation on a segment file failed.
    ///
    /// Wraps the underlying [`tokio::io::Error`]. Common causes include disk
    /// full, permission errors, or hardware failures. All such errors produce
    /// a `500 Internal Server Error` without exposing the OS error message to
    /// the client.
    #[error("Failed to open or write to ledger segment")]
    IoError(#[from] tokio::io::Error),

    /// The requested segment file does not exist on disk.
    ///
    /// Returned by [`Handle::read_segment`] when the `file_name` argument
    /// points to a path that does not exist in the user's ledger directory.
    /// Produces a `404 Not Found` HTTP response.
    ///
    /// [`Handle::read_segment`]: crate::storage::ledger::handle::Handle::read_segment
    #[error("Requested delta segment doesn't exist")]
    SegmentNotFound,

    /// The requested read offset exceeds the length of the segment file.
    ///
    /// Returned by [`Handle::read_segment`] when the provided `offset` (after
    /// applying the 500-byte floor) is beyond the end of the file. Produces a
    /// `400 Bad Request` HTTP response.
    ///
    /// [`Handle::read_segment`]: crate::storage::ledger::handle::Handle::read_segment
    #[error("Requested offset is out of bounds")]
    InvalidOffset,

    /// The `file_name` argument failed security validation.
    ///
    /// Returned by [`Handle::read_segment`] if the filename contains a path
    /// separator (`/` or `\`) or does not start with the required `"delta_"`
    /// prefix. Produces a `400 Bad Request` HTTP response.
    ///
    /// [`Handle::read_segment`]: crate::storage::ledger::handle::Handle::read_segment
    #[error("Invalid File Name")]
    InvalidFileName,

    /// A prune request attempted to delete the active segment or a segment
    /// with an ID beyond the current active segment.
    ///
    /// The `before` value in the request was greater than the ID of the
    /// currently active segment. Producing a `400 Bad Request` HTTP response.
    ///
    /// This check prevents a client from wiping a segment that is currently
    /// being written to or that does not exist yet.
    #[error("Cannot prune active or future segments")]
    InvalidPrune,
}

use Error::{
    CommitterDead, CorruptedSegment, InvalidFileName, InvalidOffset,
    InvalidPrune, IoError, ParseIntFailure, SegmentNotFound,
};

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            SegmentNotFound => {
                (StatusCode::NOT_FOUND, "Requested delta log doesn't exist")
                    .into_response()
            }
            InvalidOffset | InvalidFileName | InvalidPrune => (
                StatusCode::BAD_REQUEST,
                "Requested offset exceeds the ledger size",
            )
                .into_response(),
            CommitterDead | CorruptedSegment(_) | ParseIntFailure(_)
            | IoError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
                    .into_response()
            }
        }
    }
}

impl Loggable for Error {
    fn log_level(&self) -> crate::logger::Level {
        match self {
            SegmentNotFound | InvalidOffset => Level::Warning,
            CommitterDead | CorruptedSegment(_) => Level::Fatal,
            _ => Level::Error,
        }
    }

    fn log_module(&self) -> crate::logger::Module {
        Module::Ledger
    }
}
