//! HTTP API layer: route handlers, authentication endpoints, and middleware.
//!
//! This module is the outermost layer of the server. Its sole responsibilities are:
//!
//! - Parsing and validating HTTP requests.
//! - Delegating business logic to the [`crate::model`] and [`crate::storage`] layers.
//! - Producing HTTP responses, including error sanitization via `Error::into_response`.
//!
//! ## Sub-modules
//!
//! - [`crate::api::assets`] — Upload, download, and delete handlers for encrypted blobs.
//! - [`crate::api::auth`] — Registration and login endpoints.
//! - [`crate::api::middleware`] — Request authentication and response telemetry.
//! - [`crate::api::route`] — The top-level Axum router that composes all of the above.
//! - [`mod@crate::api::error`] — The `api::Error` type that covers all HTTP-layer failures.
/// Asset upload, paginated listing, streaming download, and deletion handlers.
pub mod assets;
pub mod auth;
/// Ingress authentication guards, MAC validation, rate limiting, and telemetry middleware.
pub mod middleware;
/// Router construction and route binding.
pub mod route;
/// Delta-CRDT WAL sync endpoints (append, stream, prune).
pub mod sync;

/// HTTP API error types and response conversion.
pub mod error;

pub use error::{Error, Result};
