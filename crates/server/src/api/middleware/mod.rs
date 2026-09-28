//! HTTP middleware components for the Kosh server.
//!
//! This module provides the middleware layers that protect the authenticated
//! and unauthenticated route groups. Each layer is a thin Axum `from_fn`
//! middleware that enforces exactly one invariant and delegates all business
//! logic to the domain layer.
//!
//! ## Ingress Funnel (Authenticated Routes `/api/v1/*`)
//!
//! ```text
//! IP Governor (global)
//!   → mac_guard     — BLAKE3 stateless token pre-filter
//!   → Device Governor (per-token)
//!   → auth_guard    — Moka cache / SQLite session resolution
//!   → Handler
//! ```
//!
//! ## Ingress Funnel (Auth Routes `/api/auth/*`)
//!
//! ```text
//! IP Governor (strict auth limit)
//!   → pow_guard     — Hashcash Proof-of-Work validator  (login / register only)
//!   → Handler
//! ```
//!
//! The [`log_middleware`] is layered above the entire router and only
//! attached when the background logger is active.
mod auth_guard;
mod log;
mod mac_guard;
mod rate_limit;

pub use auth_guard::auth_guard;
pub use log::log_middleware;
pub use mac_guard::mac_guard;
pub use rate_limit::RateLimitExt;
