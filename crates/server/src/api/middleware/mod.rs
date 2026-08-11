//! HTTP middleware components for the Kosh server.
//!
//! This module exposes two middleware functions that wrap the protected route layer:
//!
//! - [`auth_guard`]: Validates the session token on every incoming request before
//!   passing it to the handler.
//! - [`log_middleware`]: Runs after the handler completes and collects error telemetry
//!   from the response extensions for dispatch to the logging service.
mod auth_guard;
mod log;

pub use auth_guard::auth_guard;
pub use log::log_middleware;
