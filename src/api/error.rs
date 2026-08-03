use axum::response::IntoResponse;
use hyper::{StatusCode, header::InvalidHeaderValue};
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
    NotFound(String),

    #[error("Invalid header value : {}", .0)]
    InvalidHeader(#[from] InvalidHeaderValue),
}

pub type Result<T> = core::result::Result<T, Error>;

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        use Error::{
            BadRequest, Internal, InvalidHeader, IoError,
            MalformedMultipart, MissingField, NotFound,
            StreamReadError, Unauthorized,
        };

        match self {
            BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            MalformedMultipart => (
                StatusCode::BAD_REQUEST,
                "Malformed Multipart Payload".into(),
            ),
            MissingField => (
                StatusCode::BAD_REQUEST,
                "Missing required field".into(),
            ),
            InvalidHeader(_) => {
                (StatusCode::BAD_REQUEST, "Invalid Header Value".into())
            }

            StreamReadError | IoError(_) | Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".into(),
            ),
        }
        .into_response()
    }
}
