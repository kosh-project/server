mod error;
mod r#macro;
mod service;

use std::sync::OnceLock;

use bincode_next::{Decode, Encode};

pub static GLOBAL_LOGGER: OnceLock<Sender<Entry>> = OnceLock::new();

#[derive(Encode, Decode)]
pub struct Entry {
    pub module: Module,
    pub level: Level,
    pub timestamp_ms: i64,
    pub message: String,
}

#[derive(Encode, Decode)]
pub enum Level {
    Info,
    Warning,
    Error,
    Fatal,
    Shutdown,
}

#[derive(Encode, Decode)]
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
