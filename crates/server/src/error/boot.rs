use kosh_core::config;
use kosh_core::tls;
use tokio::io;

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

    // #[error("Server Failure: {}", .0)]
    // Server(#[from] Box<dyn StdErr>),
    #[error("Logger failed to boot: {}", .0)]
    Logger(String),

    #[error("Migration failure: {}", .0)]
    Migration(#[from] sqlx::migrate::MigrateError),
}
