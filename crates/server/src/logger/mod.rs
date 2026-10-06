/// Internal macro utilities.
pub mod macro_rules {
    // we use a nested module to avoid naming collision with reserved keyword macro
}
mod error;
/// Trait mapping error types to log levels and subsystem modules without string formatting.
pub mod loggable;
/// Ergonomic logging macros (`info!`, `warn!`, `error!`, `fatal!`, `shutdown!`).
pub mod macros;
pub use error::{Error, Result};
/// Background logging service, bincode file writer, and UDS broadcaster.
pub mod service;

pub use kosh_core::logger::{Entry, Level, Module};
pub use loggable::Loggable;
pub use service::{Service, format_date_time};

use std::sync::OnceLock;
use tokio::sync::mpsc::Sender;

/// Global channel sender used to enqueue structured log entries to the background logger service.
pub static GLOBAL_LOGGER: OnceLock<Sender<Entry>> = OnceLock::new();

/// Returns `true` if the global logger service has been initialized and is actively listening.
#[inline]
pub fn logging_enabled() -> bool {
    GLOBAL_LOGGER.get().is_some()
}

#[cfg(test)]
mod test;
