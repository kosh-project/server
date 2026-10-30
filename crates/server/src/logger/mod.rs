//! Structured telemetry logging pipeline, bincode daily file persistence, and Unix Datagram Socket broadcaster.
//!
//! # Architecture
//!
//! Kosh uses an asynchronous, non-blocking telemetry architecture designed for zero disk I/O on
//! request-handling threads:
//!
//! 1. **Call sites** emit structured [`kosh_core::logger::Entry`] records using ergonomic macros ([`crate::info!`],
//!    [`crate::warn!`], [`crate::error!`], etc.).
//! 2. Entries are enqueued into a bounded MPSC channel held by the [`GLOBAL_LOGGER`](crate::logger::GLOBAL_LOGGER) static [`std::sync::OnceLock`].
//! 3. A dedicated background [`Service`](crate::logger::Service) task consumes incoming entries from the channel,
//!    serializes them with `bincode`, and appends them to date-partitioned binary files in `<vault_path>/state/logs/`.
//! 4. Simultaneously, each serialized [`kosh_core::logger::Telemetry`] frame is broadcast over an
//!    unbound Unix Datagram Socket to the path specified in [`kosh_core::config::Config::socket_path`] (typically `/tmp/kosh.sock`)
//!    for consumption by the real-time admin CLI dashboard (`kosh-cli`).
//!
//! # Graceful Teardown
//!
//! During server shutdown, a `Shutdown` level entry is emitted to the channel, signalling the service loop
//! to exit after committing pending logs. The caller awaits [`Handle::shutdown_with_grace`](crate::logger::Handle::shutdown_with_grace) to ensure
//! all in-flight entries are flushed before process termination.

/// Internal macro utilities.
pub mod macro_rules {
    // we use a nested module to avoid naming collision with reserved keyword macro
}
mod error;

mod handle;
pub use handle::Handle;

/// Trait mapping error types to log levels and subsystem modules without string formatting.
pub mod loggable;
/// Ergonomic logging macros (`info!`, `warn!`, `error!`, `fatal!`, `shutdown!`).
pub mod macros;
pub use error::{Error, Result};
/// Background logging service, bincode file writer, and UDS broadcaster.
pub mod service;

use kosh_core::config::Config;
pub use kosh_core::logger::{Entry, Level, Module};
pub use loggable::Loggable;
pub use service::{Service, format_date_time};

use std::sync::OnceLock;
use tokio::sync::mpsc::Sender;

use crate::error::boot::{self, Error::AlreadyInitiated};

/// Global channel sender used to enqueue structured log entries to the background logger service.
pub static GLOBAL_LOGGER: OnceLock<Sender<Entry>> = OnceLock::new();

/// Returns `true` if the global logger service has been initialized and is actively listening.
#[inline]
pub fn logging_enabled() -> bool {
    GLOBAL_LOGGER.get().is_some()
}

#[cfg(test)]
mod test;

/// Initializes the background logger service and sets the global logger sender.
///
/// Starts the [`Service`] background worker with a bounded channel, binds the Unix datagram
/// socket for CLI telemetry broadcasting, and stores the sender in [`GLOBAL_LOGGER`].
///
/// # Arguments
///
/// * `config` - Server configuration specifying log directories and Unix socket paths.
///
/// # Errors
///
/// - Returns [`boot::Error::GlobalLogger`] if creating the log directory, opening the initial log file,
///   or creating the Unix socket fails.
/// - Returns [`boot::Error::AlreadyInitiated`] if [`GLOBAL_LOGGER`] has already been populated.
pub async fn init(config: &Config) -> boot::Result<Handle> {
    let (sender, handle) = Service::start(config).await?;

    GLOBAL_LOGGER
        .set(sender)
        .map_err(|_| AlreadyInitiated("Global Logger"))?;

    Ok(handle)
}
