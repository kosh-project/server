//! Kosh — a zero-knowledge self-hosted storage server.
//!
//! This crate is the main library for the Kosh server. It is structured as a
//! library so that integration tests can import and construct the full application
//! without duplicating any setup code.
//!
//! ## Module overview
//!
//! - [`api`] — HTTP route handlers, middleware, and API-layer error types.
//! - [`app`] — Application state and the builder used to construct it.
//! - [`mod@error`] — The top-level error type and its domain-specific sub-modules.
//! - [`logger`] — Structured asynchronous telemetry: MPSC channel, background service,
//!   daily rolling log files, Unix Datagram Socket broadcasting, and the logging macros.
//! - [`mod@log`] — A lightweight, deprecated debug-only logging macro and ANSI color helpers.
//!   Superseded by the structured macros in [`logger`].
//! - [`model`] — Database entity definitions and query functions (users, sessions, assets).
//! - [`storage`] — The CAS storage engine, file transactions, and blob management.
//!
//! ## Architecture
//!
//! ```text
//! HTTP Request
//!     ↓
//! log_middleware (post-response: collects telemetry Entry from Response extensions)
//!     ↓
//! Layer 1: Global IP Governor (tower_governor: 25 req/s, burst 10 per IP)
//!     ↓
//! ┌──────────────────────────────────────┬───────────────────────────────────────────┐
//! │ Auth Routes (/api/auth/*)            │ Protected Routes (/api/v1/*)              │
//! │                                      │                                           │
//! │ Auth IP Governor (2 req/s per IP)    │ Layer 2: mac_guard (Stateless BLAKE3 MAC) │
//! │     ↓                                │     ↓                                     │
//! │ pow_guard (Hashcash PoW validation)  │ Layer 3: Device Governor (15 req/s token) │
//! │     ↓                                │     ↓                                     │
//! │ Auth Handlers (register, login)      │ Layer 4: auth_guard (Moka cache / SQLite) │
//! └──────────────────────────────────────┴─────────────────────┬─────────────────────┘
//!                                                              ↓
//!                                                Protected Handlers
//!                                                (CAS Storage Vault, WAL Ledger, Assets)
//!                                                              ↓
//! crate::Error::into_response() — zero-allocation error sanitization + telemetry packing
//!                                                              ↓
//! HTTP Response
//!
//! Telemetry pipeline (parallel, non-blocking):
//! crate::Error::into_response()
//!     → inserts Entry into Response::extensions
//!     → log_middleware extracts Entry and calls GLOBAL_LOGGER.try_send(entry)
//!     → Service::run() receives entry, writes bincode to daily .bin file
//!     → broadcasts same bytes to /tmp/kosh.sock (kosh-cli admin dashboard)
//! ```

/// HTTP route handlers, ingress middleware, and API error sanitization.
pub mod api;
/// Application state, database pool management, and dependency injection.
pub mod app;
/// Deprecated debug logging macros and ANSI terminal formatting.
pub mod log;
/// SQLite domain entities, relations, and async query models.
pub mod model;
/// CAS blob vault, upload transaction physics, and WAL delta ledger engine.
pub mod storage;

/// Centralized application error handling and domain propagation.
pub mod error;

pub use error::{Error, Result};

/// Bearer token format definitions and cryptographic secrets management.
pub mod auth;
/// Structured telemetry pipeline, daily bincode writer, and socket broadcaster.
pub mod logger;
/// Production Axum server runner supporting plain HTTP and Rustls HTTPS.
pub mod server;
