use std::num::TryFromIntError;

use axum::response::IntoResponse;
use hyper::StatusCode;

use crate::{error::internal, wrap_internal_err};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Database Error : {}", .0)]
    Database(#[from] sqlx::Error),

    #[error("Asset not found")]
    AssetNotFound,

    #[error("Internal Err {}", .0)]
    Internal(#[from] internal::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

wrap_internal_err! {
    TryFromIntError => Error::Internal
}


impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {

        use Error::{AssetNotFound, Database, Internal};

        match self {
           AssetNotFound => (StatusCode::NOT_FOUND, "Asset not found in database"),

           Database(_) | Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
        }.into_response()
    }
}