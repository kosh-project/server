use bincode_next::{Decode, Encode};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Level {
    Info = 0,
    Warning = 1,
    Error = 2,
    Fatal = 3,
    Shutdown = 4,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Module {
    Server = 0,
    Database = 1,
    Storage = 2,
    Asset = 3,
    Api = 5,
    Ledger = 6,
    Logger = 7,
}

#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct Entry {
    pub module: Module,
    pub level: Level,
    pub timestamp_ms: i64,
    pub message: String,
}

#[derive(Debug, Encode, Decode, Serialize, Deserialize)]
pub enum Telemetry {
    Log(Entry),
    Heartbeat,
}
