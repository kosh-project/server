use std::num::TryFromIntError;
use std::time::SystemTimeError;

use axum::response::IntoResponse;
use hyper::StatusCode;

use crate::api;
use crate::storage;
use crate::{model, wrap_internal_err};

pub mod internal;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    StorageError(#[from] storage::Error),

    #[error("API Error : {}", .0)]
    ApiError(#[from] api::Error),

    #[error("User Conflict")]
    Conflict(String),

    #[error("Database Error : {}", .0)]
    DatabaseError(#[from] sqlx::Error),

    #[error("Internal Error : {}", .0)]
    InternalError(#[from] internal::Error),

    #[error("Model Error : {}", .0)]
    ModelError(#[from] model::Error)
}

wrap_internal_err! {
    TryFromIntError, SystemTimeError => Error::InternalError
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string())
            .into_response()
    }
}

pub type Result<T> = core::result::Result<T, Error>;
