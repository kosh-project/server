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
/// # Errors
///
/// Returns [`boot::Error::Logger`] if starting the logger service fails.
/// Returns [`boot::Error::AlreadyInitiated`] if the global logger was already set.
pub async fn init(config: &Config) -> boot::Result<Handle> {
    let (sender, handle) = Service::start(config).await?;

    GLOBAL_LOGGER
        .set(sender)
        .map_err(|_| AlreadyInitiated("Global Logger"))?;

    Ok(handle)
}
