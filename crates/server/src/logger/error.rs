#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to write to log file: {}", .0)]
    Io(#[from] std::io::Error),

    #[error("Failed to serialzie log event: {}", .0)]
    Serialization(#[from] bincode_next::error::EncodeError),

    #[error("Failed to initialize log directory")]
    LogDirectoryInitialization,
}

pub type Result<T> = std::result::Result<T, Error>;