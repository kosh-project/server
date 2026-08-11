//! Database entity definitions and query functions.
//!
//! This module contains the data model for every entity stored in the SQLite database.
//! Each sub-module corresponds to one table and owns the query functions that operate
//! on that table. All query functions accept a [`sqlx::SqlitePool`] reference rather than
//! taking ownership, so they can be called freely from any async context.
//!
//! ## Sub-modules
//!
//! - [`asset`] — The `assets` table. Tracks which users own which blobs (CAS hashes).
//!   Implements reference-counted deletion: the physical file is only removed when the
//!   last ownership row is deleted.
//! - [`session`] — The `sessions` table. Manages opaque session tokens with a 30-day TTL.
//! - [`user`] — The `users` table. Stores hashed identities and authentication verifiers.
//! - [`error`] — The `model::Error` type covering all database-layer failures.
pub mod asset;
pub mod error;
pub mod session;
pub mod user;

pub use error::{Error, Result};
