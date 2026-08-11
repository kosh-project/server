//! HTTP API layer: route handlers, authentication endpoints, and middleware.
//!
//! This module is the outermost layer of the server. Its sole responsibilities are:
//!
//! - Parsing and validating HTTP requests.
//! - Delegating business logic to the [`model`] and [`storage`] layers.
//! - Producing HTTP responses, including error sanitization via [`error::Error::into_response`].
//!
//! ## Sub-modules
//!
//! - [`assets`] — Upload, download, and delete handlers for encrypted blobs.
//! - [`auth`] — Registration and login endpoints.
//! - [`middleware`] — Request authentication and response telemetry.
//! - [`route`] — The top-level Axum router that composes all of the above.
//! - [`error`] — The `api::Error` type that covers all HTTP-layer failures.
//!
//! [`model`]: crate::model
//! [`storage`]: crate::storage
pub mod assets;
pub mod auth;
pub mod middleware;
pub mod route;

pub mod error;

pub use error::{Error, Result};
