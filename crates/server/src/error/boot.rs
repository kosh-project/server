use kosh_core::config;
use kosh_core::tls;
use tokio::io;

use crate::logger;

#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum Error {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] config::Error),

    #[error("Io Err: {}", .0)]
    Io(#[from] io::Error),

    #[error("Database error: {}", .0)]
    Database(#[from] sqlx::Error),

    #[error("TLS Error: {}", .0)]
    Tls(#[from] tls::Error),

    #[error("Logger failed to boot: {}", .0)]
    GlobalLogger(#[from] logger::Error),

    #[error("{} already initiated", .0)]
    AlreadyInitiated(&'static str),

    #[error("Migration failure: {}", .0)]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Server secret corrupted: {}", .0)]
    Secret(String),
}

pub type Result<T> = core::result::Result<T, Error>;
