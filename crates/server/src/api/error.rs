use std::sync::Arc;

use axum::response::IntoResponse;
use hyper::{StatusCode, header::InvalidHeaderValue};
use tokio::io;

use crate::{error::internal, logger::Loggable};

/// Errors that can occur while processing an HTTP API request.
///
/// These are split into two categories:
///
/// - **Client errors (4xx):** Safe to return directly to the caller. They indicate
///   the client sent something malformed or unauthorized.
/// - **Internal errors (5xx):** Must never expose implementation details. The `IntoResponse`
///   implementation strips these down to a generic message before sending.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The multipart form body was structurally invalid and could not be parsed.
    #[error("Malformed Multipart found")]
    MalformedMultipart,

    /// The client sent a request that violates the API contract.
    /// The inner `String` contains a human-readable explanation safe to send back.
    #[error("Bad Request")]
    BadRequest(String),

    /// Reading the request body stream failed mid-transfer.
    /// This is typically a network issue on the client side.
    #[error("Stream Read Error")]
    StreamReadError,

    /// A required field was absent from the request body or headers.
    #[error("Missing Field")]
    MissingField,

    /// An I/O error occurred while processing the request, such as writing a
    /// temporary file to disk. This is an internal failure.
    #[error("IO Error : {}", .0)]
    IoError(#[from] io::Error),

    /// The request could not be authenticated. The inner `String` contains
    /// a message safe to return (e.g., "Missing Header", "Invalid token").
    #[error("Unauthorized : {}", .0)]
    Unauthorized(String),

    /// A low-level internal error, typically from integer or time conversions.
    #[error("Internal Server Error : {}", .0)]
    Internal(#[from] internal::Error),

    /// The requested resource does not exist or is not accessible to the caller.
    /// The inner `String` contains a message safe to return to the client.
    #[error("Not Found")]
    NotFound(String),

    /// A header value provided in the response could not be parsed.
    /// This is almost always a bug in the server code, not the client.
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

        let mut response = match &*self_arc {
            // Client errors: safe to return the message directly.
            BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, msg.clone())
            }
            NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
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

            // Internal errors: strip all details before sending.
            StreamReadError | IoError(_) | Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".into(),
            ),
        }
        .into_response();

        response.extensions_mut().insert(self_arc.clone());

        response
    }
}

use crate::logger::{Level, Module};
impl Loggable for Error {
    fn log_level(&self) -> Level {
        use Error::*;
        match self {
            BadRequest(_) | Unauthorized(_) | MissingField => {
                Level::Warning
            }
            NotFound(_) => Level::Info,
            _ => Level::Error,
        }
    }

    #[inline]
    fn log_module(&self) -> Module {
        Module::Api
    }
}
