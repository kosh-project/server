mod error;
mod r#macro;
mod service;

use std::sync::OnceLock;

use bincode_next::{Decode, Encode};
mod loggable;

pub use loggable::Loggable;

pub static GLOBAL_LOGGER: OnceLock<Sender<Entry>> = OnceLock::new();

#[inline]
pub fn logging_enabled() -> bool {
    GLOBAL_LOGGER.get().is_some()
}

#[derive(Encode, Decode, Clone)]
pub struct Entry {
    pub module: Module,
    pub level: Level,
    pub timestamp_ms: i64,
    pub message: String,
}

#[derive(Encode, Decode, Clone, Copy)]
pub enum Level {
    Info,
    Warning,
    Error,
    Fatal,
    Shutdown,
}

#[derive(Encode, Decode, Clone, Copy)]
pub enum Module {
    Api,
    Database,
    Server,
    Asset,
    Storage,
    Logger,
}

pub use service::Service;
use tokio::sync::mpsc::Sender;
