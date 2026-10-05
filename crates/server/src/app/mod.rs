//! Application state and its builder.
//!
//! This module provides [`State`](crate::app::State), the shared application context that Axum injects
//! into every route handler, and [`AppStateBuilder`](crate::app::AppStateBuilder), the fluent builder used to
//! construct it during startup and in tests.
//!
//! ## Design note
//!
//! `State` is intentionally cheap to clone: the `SqlitePool` and `session_cache` both
//! use `Arc`-backed shared ownership internally, and `Service` is a thin wrapper over
//! a `PathBuf`. Cloning `State` is therefore an O(1) reference-count increment.
/// Shared application state injected into Axum route handlers.
pub mod state;
pub use state::State;
mod builder;

pub use builder::AppStateBuilder;
