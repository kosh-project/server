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
/// - [`mod@log`] — A lightweight debug-only logging macro and ANSI color helpers.
/// - [`model`] — Database entity definitions and query functions (users, sessions, assets).
/// - [`storage`] — The CAS storage engine, file transactions, and blob management.
///
/// ## Architecture
///
/// ```text
/// HTTP Request
///     ↓
/// auth_guard middleware  (Moka cache → SQLite fallback)
///     ↓
/// Route Handler (api layer)
///     ↓
/// Model Layer  (sqlx queries against SQLite)
/// Storage Layer  (streaming to disk, CAS vault)
///     ↓
/// crate::Error::into_response() — central error sanitization
///     ↓
/// HTTP Response
/// ```
pub mod api;
pub mod app;
pub mod log;
pub mod model;
pub mod storage;

pub mod error;

pub use error::{Error, Result};
