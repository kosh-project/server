#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Database Error : {}", .0)]
    Database(#[from] sqlx::Error),

    #[error("Asset not found")]
    AssetNotFound,
}

pub type Result<T> = std::result::Result<T, Error>;
