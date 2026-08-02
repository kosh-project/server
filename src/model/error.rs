use std::num::TryFromIntError;

use crate::{error::internal, wrap_internal_err};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Database Error : {}", .0)]
    Database(#[from] sqlx::Error),

    #[error("Asset not found")]
    AssetNotFound,

    #[error("Internal Err {}", .0)]
    Internal(#[from] internal::Error)
}

pub type Result<T> = std::result::Result<T, Error>;


wrap_internal_err! {
    TryFromIntError => Error::Internal
}