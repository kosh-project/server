pub mod macro_rules {
    // we use a nested module to avoid naming collision with reserved keyword macro
}
mod error;
pub mod loggable;
pub mod macros;
pub use error::{Error, Result};
pub mod service;

pub use kosh_core::logger::{Entry, Level, Module};
pub use loggable::Loggable;
pub use service::{Service, format_date_time};

use std::sync::OnceLock;
use tokio::sync::mpsc::Sender;

pub static GLOBAL_LOGGER: OnceLock<Sender<Entry>> = OnceLock::new();

#[inline]
pub fn logging_enabled() -> bool {
    GLOBAL_LOGGER.get().is_some()
}
