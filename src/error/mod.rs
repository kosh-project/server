use crate::api::Error as ApiErr;
use crate::storage::Error as StorageErr;
use std::io::Error as IoErr;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    StorageError(#[from] StorageErr),
    #[error("API Error : {}", .0)]
    ApiError(#[from] ApiErr),
}
