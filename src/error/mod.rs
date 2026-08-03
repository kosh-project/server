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
    ModelError(#[from] model::Error),
}

wrap_internal_err! {
    TryFromIntError, SystemTimeError => Error::InternalError
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        use Error::{
            ApiError, Conflict, DatabaseError, InternalError,
            ModelError, StorageError,
        };

        match self {
            StorageError(error) => error.into_response(),
            ApiError(error) => error.into_response(),
            ModelError(error) => error.into_response(),

            Conflict(error) => {
                (StatusCode::CONFLICT, error).into_response()
            }

            DatabaseError(_) | InternalError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error",
            )
                .into_response(),
        }
    }
}

pub type Result<T> = core::result::Result<T, Error>;
