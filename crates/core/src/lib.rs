//! Shared foundational library for the Kosh self-hosted storage platform.
//!
//! This crate provides the types and abstractions consumed by both the server binary
//! (`webdav-server`) and the admin CLI (`kosh-cli`). Placing these types in a shared
//! library avoids duplicating code between the two binaries and ensures the CLI can
//! deserialise the exact same wire formats the server serialises.
//!
//! # Modules
//!
//! - [`config`] — KDL-based server configuration parsing via `knuffel`.
//! - [`logger`] — Shared log entry and telemetry types used by the structured logging
//!   pipeline and the Unix Domain Socket broadcast channel.
//! - [`tls`] — Self-signed X.509 certificate generation, loading, and SHA-256
//!   fingerprinting for TOFU device pairing.

pub mod config;
pub mod logger;
pub mod tls;
