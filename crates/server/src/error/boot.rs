use std::error::Error as StdErr;

use tokio::io;

use crate::tls;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] knuffel::Error),

    #[error("Io Err: {}", .0)]
    Io(#[from] io::Error),

    #[error("Database error: {}", .0)]
    Database(#[from] sqlx::Error),

    #[error("TLS Error: {}", .0)]
    Tls(#[from] tls::Error),

    // #[error("Server Failure: {}", .0)]
    // Server(#[from] Box<dyn StdErr>),
    #[error("Logger failed to boot: {}", .0)]
    Logger(String),
}
