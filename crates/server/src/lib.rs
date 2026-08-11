/// Kosh — a zero-knowledge self-hosted storage server.
///
/// This crate is the main library for the Kosh server. It is structured as a
/// library so that integration tests can import and construct the full application
/// without duplicating any setup code.
///
/// ## Module overview
///
/// - [`api`] — HTTP route handlers, middleware, and API-layer error types.
/// - [`app`] — Application state and the builder used to construct it.
/// - [`error`] — The top-level error type and its domain-specific sub-modules.
/// - [`logger`] — Structured asynchronous telemetry: MPSC channel, background service,
///   daily rolling log files, Unix Datagram Socket broadcasting, and the logging macros.
/// - [`mod@log`] — A lightweight, deprecated debug-only logging macro and ANSI color helpers.
///   Superseded by the structured macros in [`logger`].
/// - [`model`] — Database entity definitions and query functions (users, sessions, assets).
/// - [`storage`] — The CAS storage engine, file transactions, and blob management.
///
/// ## Architecture
///
/// ```text
/// HTTP Request
///     ↓
/// log_middleware  (post-response — collects error telemetry from extensions)
///     ↓
/// auth_guard middleware  (Moka cache → SQLite fallback)
///     ↓
/// Route Handler (api layer)
///     ↓
/// Model Layer  (sqlx queries against SQLite)
/// Storage Layer  (streaming to disk, CAS vault)
///     ↓
/// crate::Error::into_response() — central error sanitization + telemetry packing
///     ↓
/// HTTP Response
///
/// Telemetry pipeline (parallel, non-blocking):
/// crate::Error::into_response()
///     → inserts Entry into Response::extensions
///     → log_middleware extracts Entry and calls GLOBAL_LOGGER.try_send(entry)
///     → Service::run() receives entry, writes bincode to daily .bin file
///     → broadcasts same bytes to /tmp/kosh-cli.sock  (kosh-cli admin dashboard)
/// ```
pub mod api;
pub mod app;
pub mod log;
pub mod logger;
pub mod model;
pub mod storage;

pub mod error;

pub use error::{Error, Result};
