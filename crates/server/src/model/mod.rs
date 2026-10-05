//! Database entity definitions and query functions.
//!
//! This module contains the data model for every entity stored in the `SQLite` database.
//! Each sub-module corresponds to one table and owns the query functions that operate
//! on that table. All query functions accept a [`sqlx::SqlitePool`] reference rather than
//! taking ownership, so they can be called freely from any async context.
//!
//! ## Sub-modules
//!
//! - [`asset`](crate::model::asset) — The `assets` table. Tracks which users own which blobs (CAS hashes).
//!   Implements reference-counted deletion: the physical file is only removed when the
//!   last ownership row is deleted.
//! - [`session`](crate::model::session) — The `sessions` table. Manages opaque session tokens with a 30-day TTL.
//! - [`user`](crate::model::user) — The `users` table. Stores hashed identities and authentication verifiers.
//! - [`error`](crate::model::error) — The `model::Error` type covering all database-layer failures.
/// Asset metadata entities, tags, and keyset cursor pagination models.
pub mod asset;
/// Database and model layer error types.
pub mod error;
/// Authenticated session records and token hashing.
pub mod session;
/// Registered user accounts and cryptographic verifiers.
pub mod user;

pub use error::{Error, Result};
