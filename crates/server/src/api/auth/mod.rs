//! Authentication handlers: challenge generation, user registration, session creation, and recovery rotation.
//!
//! The registration, login, and challenge endpoints are unauthenticated and mounted under `/api/auth`.
//! The recovery rotation endpoint is protected behind `mac_guard` and `auth_guard` and mounted under `/api/v1/auth`.
//!
//! ## Endpoint Overview
//!
//! | Method | Path | Handler | `PoW` Required | Authenticated |
//! |--------|------|---------|----------------|---------------|
//! | `GET`  | `/api/auth/challenge` | [`challenge::generate()`] | No  | No |
//! | `POST` | `/api/auth/login`    | [`login()`]               | Yes | No |
//! | `POST` | `/api/auth/register` | [`register()`]            | Yes | No |
//! | `PUT`  | `/api/v1/auth/recovery` | [`rotate()`]           | No  | Yes |
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
mod recovery;
mod register;

pub use login::login;
pub use pow_guard::pow_guard;
pub use recovery::rotate;
pub use register::register;

#[cfg(test)]
mod test;
