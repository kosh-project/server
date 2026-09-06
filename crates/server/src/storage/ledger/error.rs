use std::num::ParseIntError;

use axum::response::IntoResponse;
use hyper::StatusCode;
use tokio::io;

use crate::logger::{Level, Loggable, Module};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Comitter died before an active sender")]
    CommitterDead,

    #[error("Corrupted segment: {}",.0)]
    CorruptedSegment(String),

    #[error(transparent)]
    ParseIntFailure(#[from] ParseIntError),

    #[error("Failed to open or write to ledger segment")]
    IoError(#[from] io::Error),

    #[error("Requested delta segment doesn't exist")]
    SegmentNotFound,

    #[error("Requested offset is out of bounds")]
    InvalidOffset,

    #[error("Invalid File Name")]
    InvalidFileName,
}

use Error::{
    CommitterDead, CorruptedSegment, InvalidFileName, InvalidOffset, IoError,
    ParseIntFailure, SegmentNotFound,
};

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            SegmentNotFound => {
                (StatusCode::NOT_FOUND, "Requested delta log doesn't exist")
                    .into_response()
            }
            InvalidOffset | InvalidFileName => (
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
