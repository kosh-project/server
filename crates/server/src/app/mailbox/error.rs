use std::sync::PoisonError;

use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
use tokio::sync::MutexGuard;

use crate::{app::mailbox::Status, error::internal};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Mailbox not found or expired")]
    NotFound,

    #[error("Mailbox already joined")]
    AlreadyJoined,

    #[error("Mailbox not ready")]
    NotReady,

    #[error("Mailbox already fulfilled")]
    AlreadyFulfilled,

    #[error("Mailbox not waiting for join")]
    NotWaiting,

    #[error("Unauthorized mailbox access")]
    Unauthorized,

    #[error("Internal mailbox error: {}", .0)]
    Internal(#[from] internal::Error),
}

pub type Result<T> = core::result::Result<T, Error>;

pub(crate) use Error::{
    AlreadyFulfilled, AlreadyJoined, Internal, NotFound, NotReady, NotWaiting,
    Unauthorized,
};

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            NotFound | Unauthorized => {
                (StatusCode::NOT_FOUND, "Mailbox not found or expired")
            }
            AlreadyJoined => {
                (StatusCode::BAD_REQUEST, "Mailbox already joined")
            }
            AlreadyFulfilled => {
                (StatusCode::BAD_REQUEST, "Mailbox already fulfilled")
            }
            NotReady => {
                (StatusCode::BAD_REQUEST, "Mailbox not ready for fulfillment")
            }
            NotWaiting => {
                (StatusCode::BAD_REQUEST, "Mailbox not wiating for join")
            }
            Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
            }
        }
        .into_response()
    }
}

impl Into<Error> for PoisonError<MutexGuard<'_, Status>> {
    fn into(self) -> Error {
        Error::Internal(internal::Error::MutexPoisoned)
    }
}
