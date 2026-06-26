use axum::response::IntoResponse;
use hyper::StatusCode;

use crate::api::Error as ApiErr;
use crate::storage::Error as StorageErr;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    StorageError(#[from] StorageErr),
    #[error("API Error : {}", .0)]
    ApiError(#[from] ApiErr),
    #[error("User Conflict")]
    Conflict(String),
    #[error("Database Error : {}", .0)]
    DatabaseError(#[from] sqlx::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
    }
}

pub type Result<T> = core::result::Result<T, Error>;
