use kosh_core::config;
use kosh_core::tls;
use tokio::io;

use crate::logger;

/// Errors that can occur during server startup and bootstrap sequence.
#[derive(thiserror::Error, miette::Diagnostic, Debug)]
pub enum Error {
    /// Failed to read or parse the KDL configuration file.
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] config::Error),

    /// Low-level filesystem I/O failure during boot.
    #[error("Io Err: {}", .0)]
    Io(#[from] io::Error),

    /// SQLite connection or database initialization failure.
    #[error("Database error: {}", .0)]
    Database(#[from] sqlx::Error),

    /// TLS certificate generation, loading, or handshake failure.
    #[error("TLS Error: {}", .0)]
    Tls(#[from] tls::Error),

    /// Telemetry logger task failed to initialize or bind its socket.
    #[error("Logger failed to boot: {}", .0)]
    GlobalLogger(#[from] logger::Error),

    /// A singleton subsystem was initialized more than once.
    #[error("{} already initiated", .0)]
    AlreadyInitiated(&'static str),

    /// SQLite database migration failed to execute against the active schema.
    #[error("Migration failure: {}", .0)]
    Migration(#[from] sqlx::migrate::MigrateError),

    /// The persistent server secret key was missing, corrupted, or unreadable.
    #[error("Server secret corrupted: {}", .0)]
    Secret(&'static str),
}

/// Convenience alias for results returned during server boot.
pub type Result<T> = core::result::Result<T, Error>;
