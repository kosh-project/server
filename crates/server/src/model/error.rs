use std::{num::TryFromIntError, time::SystemTimeError};

use axum::response::IntoResponse;
use hyper::StatusCode;

use crate::{
    error::internal,
    logger::{Level, Loggable, Module},
    model::Error::{CheckViolation, ForeignKeyViolation, UniqueViolation},
    wrap_internal_err,
};

/// Errors that originate from the model layer (database queries and data mapping).
///
/// These errors sit between the raw database driver and the rest of the application.
/// Most are internal failures from `sqlx` that should never leak to the HTTP client.
/// The `IntoResponse` implementation ensures only safe, generic messages are returned.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A `sqlx` database error. This covers connection failures, constraint violations,
    /// malformed queries, and any other error from the `SQLite` driver.
    #[error("Database Error : {}", .0)]
    Database(sqlx::Error),

    /// The requested asset was not found in the database.
    ///
    /// This is distinct from a storage-layer `NotFound`. This variant means the asset
    /// record does not exist in the `assets` table, not necessarily that the file is
    /// missing from disk.
    #[error("Asset not found")]
    AssetNotFound,

    /// A low-level internal error, typically from integer or time conversions.
    #[error("Internal Err {}", .0)]
    Internal(#[from] internal::Error),

    /// A unique constraint or primary key collision in SQLite (e.g. duplicate user identity).
    /// Sanitized into `409 Conflict` at the HTTP layer.
    #[error("Entity already exists")]
    UniqueViolation,

    /// A foreign key constraint violation (e.g. referencing a non-existent parent user).
    /// Sanitized into `400 Bad Request` at the HTTP layer.
    #[error("Referenced entity doesn't exist")]
    ForeignKeyViolation,

    /// A column or table `CHECK` constraint violation (e.g. payload length exceeding limit).
    /// Sanitized into `400 Bad Request` at the HTTP layer.
    #[error("Check violation")]
    CheckViolation,
}

impl From<sqlx::Error> for Error {
    fn from(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(ref err) = err {
            if err.is_check_violation() {
                return Self::CheckViolation;
            }
            if err.is_foreign_key_violation() {
                return ForeignKeyViolation;
            }
            if err.is_unique_violation() {
                return Self::UniqueViolation;
            }
        }

        Self::Database(err)
    }
}

/// Convenience alias for results returned by model and database operations.
pub type Result<T> = std::result::Result<T, Error>;

wrap_internal_err! {
    TryFromIntError, SystemTimeError => Error::Internal
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        use Error::{AssetNotFound, Database, Internal};

        match self {
            // Client-facing: safe to expose.
            AssetNotFound => {
                (StatusCode::NOT_FOUND, "Asset not found in database")
            }
            UniqueViolation => {
                (StatusCode::CONFLICT, "Resource already exists")
            }
            ForeignKeyViolation => {
                (StatusCode::BAD_REQUEST, "Referenced entity doesn't exist")
            }
            CheckViolation => {
                (StatusCode::BAD_REQUEST, "Entity violates constraints")
            }

            // Internal: hide the SQL details from the client.
            Database(_) | Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
            }
        }
        .into_response()
    }
}

impl Loggable for Error {
    fn log_level(&self) -> crate::logger::Level {
        use Error::{AssetNotFound, Database, Internal};
        match self {
            UniqueViolation | ForeignKeyViolation | CheckViolation => {
                Level::Warning
            }
            AssetNotFound => Level::Info,
            Internal(_) => Level::Fatal,
            Database(_) => Level::Error,
        }
    }

    #[inline]
    fn log_module(&self) -> crate::logger::Module {
        Module::Database
    }
}
