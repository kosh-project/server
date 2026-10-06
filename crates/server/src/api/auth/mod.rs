//! Authentication handlers: challenge generation, user registration, and session creation.
//!
//! All endpoints in this module are unauthenticated — they sit outside the
//! `mac_guard` and `auth_guard` middleware layers and are mounted under `/api/auth`.
//!
//! ## Endpoint Overview
//!
//! | Method | Path | Handler | `PoW` Required |
//! |--------|------|---------|--------------|
//! | `GET`  | `/api/auth/challenge` | [`challenge::generate()`] | No  |
//! | `POST` | `/api/auth/login`    | [`login()`]               | Yes |
//! | `POST` | `/api/auth/register` | [`register()`]            | Yes |
//!
//! The `pow_guard` middleware enforces the Hashcash Proof-of-Work protocol on
//! the `login` and `register` routes. The `challenge` endpoint is intentionally
//! excluded — it generates the puzzle the client must solve.
//!
//! This module operates on the [`User`] and [`Session`] domain models.
//!
//! [`User`]: crate::model::user::User
//! [`Session`]: crate::model::session::Session
/// Stateless Hashcash Proof-of-Work challenge generation.
pub mod challenge;
pub(crate) mod hashcash;
pub use hashcash::HashCash;
mod login;
mod pow_guard;
mod register;

pub use login::login;
pub use pow_guard::pow_guard;
pub use register::register;
