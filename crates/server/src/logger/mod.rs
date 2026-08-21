//! Structured, asynchronous telemetry for the Kosh server.
//!
//! This module provides the entire logging infrastructure used by the server at runtime.
//! It is intentionally decoupled from the HTTP request path: the server emits log entries
//! through an in-memory channel, and a dedicated background task handles all blocking
//! I/O (disk writes, Unix socket broadcasts) independently.
//!
//! ## Architecture
//!
//! ```text
//! Route Handler / Middleware
//!         │
//!         │  crate::info!(Module::Api, "...")  ← zero-cost if logger is inactive
//!         │
//!         ▼
//! GLOBAL_LOGGER (OnceLock<Sender<Entry>>)
//!         │
//!         │  mpsc channel  (bounded, non-blocking try_send)
//!         │
//!         ▼
//! Service::run()  ← dedicated tokio task
//!     ├── writes bincode-serialized Entry to a rolling daily .bin file
//!     └── broadcasts the same bytes over a Unix Datagram Socket
//!             └── consumed by the Ratatui admin CLI (kosh-cli)
//! ```
//!
//! ## Enabling the logger
//!
//! The logger is opt-in. Call [`Service::start`] on startup and store the returned
//! sender in [`GLOBAL_LOGGER`]. If `GLOBAL_LOGGER` is never initialized, all macros
//! (`info!`, `warn!`, `error!`, `fatal!`) become complete no-ops with zero overhead.
//!
//! ## Emitting log entries
//!
//! Use the crate-level macros exported from [`macro`](crate) rather than constructing
//! [`Entry`] values manually:
//!
//! ```ignore
//! info!(Module::Api, "user {} logged in", user_id);
//! warn!(Module::Storage, "disk usage above 90%");
//! error!(Module::Database, "query failed: {}", e);
//! ```
mod error;
mod r#macro;
mod service;

use std::path::PathBuf;
use std::sync::OnceLock;

use bincode_next::{Decode, Encode};
mod loggable;

pub use loggable::Loggable;
pub use service::{SOCKET_ADDR, format_date_time};

/// The global channel sender used to submit log entries to the background logging service.
///
/// Initialized once on startup by calling [`Service::start`] and storing the returned sender
/// here via `OnceLock::set`. Once set, this value is immutable for the lifetime of the process.
///
/// The sender is intentionally stored as an `OnceLock` rather than a separate `AtomicBool` flag
/// so that there is a single source of truth for whether logging is active. Code that needs to
/// check whether logging is enabled should call [`logging_enabled`] instead of reading this
/// directly.
pub static GLOBAL_LOGGER: OnceLock<Sender<Entry>> = OnceLock::new();

/// Returns `true` if the logging service has been initialized and is currently active.
///
/// This is the canonical way to check whether logging is enabled before performing any
/// work related to telemetry. It reads from [`GLOBAL_LOGGER`] and avoids the need for
/// a separate `AtomicBool` flag.
///
/// Used in `IntoResponse` to decide whether to construct an [`Entry`] for the error
/// telemetry middleware, and in `route_main` to decide whether to attach the logging
/// middleware layer.
#[inline]
pub fn logging_enabled() -> bool {
    GLOBAL_LOGGER.get().is_some()
}

/// A single structured log event emitted by the server.
///
/// `Entry` is the wire format for all telemetry in the system. It is serialized using
/// `bincode` before being written to disk or broadcast over the Unix Datagram Socket.
/// The layout is intentionally compact: `Module` and `Level` are stored as small integer
/// enums, keeping each entry small for high-throughput workloads.
///
/// Entries are constructed by the logging macros (`info!`, `error!`, etc.) and by the
/// error telemetry middleware in `api/middleware/log.rs`. They should not typically be
/// constructed manually.
#[derive(Encode, Decode, Clone)]
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
    ///
    /// For error telemetry entries produced by `IntoResponse`, this is the result of
    /// calling `.to_string()` on the error, which uses the `Display` format defined
    /// by `thiserror`.
    pub message: String,
}

/// The severity level of a log entry.
///
/// Levels are assigned by each error type through the [`Loggable`] trait, which allows
/// individual domain errors to decide their own severity without requiring a centralized
/// `match` statement in the middleware.
///
/// The admin CLI (`kosh-cli`) uses these levels to apply color coding and filtering
/// when rendering the log feed.
#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Routine informational events: successful logins, uploads, health checks.
    Info,
    /// Events that are abnormal but non-fatal: client authentication failures,
    /// request conflicts, or resource not found responses.
    Warning,
    /// Unexpected failures that affect a single request but do not crash the service:
    /// database query errors, storage write failures.
    Error,
    /// Critical failures that indicate the server may be in an unrecoverable state:
    /// vault directory missing, logger task crash.
    Fatal,
    /// A special sentinel level used to stop the logging service gracefully.
    ///
    /// When the service receives an entry with this level, it stops its receive loop
    /// and allows the `LoggerHandler` to join cleanly. This is the "poison pill"
    /// pattern used instead of relying on channel closure, because the sender lives
    /// inside a `OnceLock` and is never dropped during normal operation.
    Shutdown,
}

/// The server subsystem that produced a log entry.
///
/// Used by the admin CLI to filter or group log entries by origin. Each domain error
/// type reports its module through the [`Loggable`] trait.
#[derive(Debug, Encode, Decode, Clone, Copy, PartialEq, Eq)]
pub enum Module {
    /// HTTP layer: route handlers, middleware, request parsing.
    Api,
    /// Database layer: `sqlx` queries, migrations, model operations.
    Database,
    /// Core server lifecycle: startup, shutdown, TCP listener.
    Server,
    /// Asset domain: upload, download, delete, and ownership operations.
    Asset,
    /// Storage layer: CAS filesystem, file transactions, blob management.
    Storage,
    /// The logging service itself. Used for internal diagnostics such as
    /// grace-period timeout warnings during shutdown.
    Logger,
}

pub use service::Service;
use tokio::sync::mpsc::Sender;

#[must_use]
pub fn path() -> Option<PathBuf> {
    dirs::state_dir().map(|x| x.join("kosh").join("logs"))
}
