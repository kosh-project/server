use bincode_next::{Decode, Encode};
use serde::{Deserialize, Serialize};

/// The severity level of a log entry.
///
/// Levels are assigned by each error type through the `Loggable` trait in the server crate,
/// allowing individual domain errors to declare their own severity without requiring a
/// centralised `match` statement in the middleware.
///
/// The admin CLI uses these levels for colour coding and filtering in the log viewer.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Level {
    /// Routine informational events: successful logins, uploads, health checks.
    Info = 0,
    /// Abnormal but non-fatal events: authentication failures, resource conflicts,
    /// or requests that result in a 4xx response.
    Warning = 1,
    /// Unexpected failures affecting a single request but not crashing the service:
    /// database query errors, storage write failures.
    Error = 2,
    /// Critical failures indicating the server may be in an unrecoverable state:
    /// vault directory missing, background task crash, or an unhandled panic.
    Fatal = 3,
    /// A special sentinel level used to stop the logging service gracefully.
    ///
    /// When the service receives an entry with this level, it exits its receive loop
    /// cleanly. This is the "poison pill" pattern used instead of relying on channel
    /// closure, because the global sender lives inside a `OnceLock` and is never
    /// dropped during normal operation.
    Shutdown = 4,
}

/// The server subsystem that produced a log entry.
///
/// Used by the admin CLI to filter or group log entries by origin. Each domain error
/// type in the server crate reports its module through the `Loggable` trait.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Module {
    /// Core server lifecycle: startup, shutdown, TCP listener.
    Server = 0,
    /// Database layer: `sqlx` queries, migrations, model operations.
    Database = 1,
    /// Storage layer: CAS filesystem, file transactions, blob management.
    Storage = 2,
    /// Asset domain: upload, download, delete, and ownership operations.
    Asset = 3,
    /// HTTP layer: route handlers, middleware, request parsing.
    Api = 5,
    /// Delta-CRDT sync ledger: segment writes, rotations, and prune operations.
    Ledger = 6,
    /// The logging service itself. Used for internal diagnostics such as
    /// grace-period timeout warnings during shutdown.
    Logger = 7,
}

/// A single structured log event emitted by the server.
///
/// `Entry` is the on-disk wire format. It is serialised using `bincode` before being
/// written to the daily `.bin` log file. The layout is intentionally compact: `Module`
/// and `Level` are stored as small integer enums, keeping each entry small for
/// high-throughput workloads.
///
/// Entries are constructed by the logging macros (`info!`, `error!`, etc.) in the server
/// crate and by the error telemetry middleware. They should not typically be constructed
/// manually outside of tests.
#[derive(Debug, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct Entry {
    /// The subsystem that generated this log entry.
    pub module: Module,
    /// The severity level of this log entry.
    pub level: Level,
    /// The Unix epoch timestamp in milliseconds at which this entry was created.
    ///
    /// The logging service uses this value (not the wall clock) to determine which
    /// daily log file to write the entry into, preventing incorrect file rotation
    /// when entries are processed slightly after midnight due to channel queue lag.
    pub timestamp_ms: i64,
    /// The human-readable log message.
    pub message: String,
}

/// The over-the-wire frame format sent from the server to the admin CLI via Unix socket.
///
/// The server's logging background task uses this enum to multiplex two types of signals
/// over the same Unix Datagram Socket:
///
/// - [`Telemetry::Log`] carries a real structured log entry.
/// - [`Telemetry::Heartbeat`] is sent every 3 seconds when no log entries have arrived,
///   allowing the CLI to distinguish an idle server from a dead one.
///
/// Note that `Entry` (the raw log event) is written directly to the `.bin` disk file in
/// the `Log` variant's payload. The `Telemetry` wrapper is only ever serialised for
/// transmission over the socket; it is never written to disk.
#[derive(Debug, Encode, Decode, Serialize, Deserialize)]
pub enum Telemetry {
    /// A real log event to be displayed in the admin CLI.
    Log(Entry),
    /// An idle keepalive signal. Receiving this proves the server process is alive
    /// and the socket connection is healthy, even when no activity is occurring.
    Heartbeat,
}
