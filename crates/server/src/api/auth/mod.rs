//! Authentication handlers: challenge generation, user registration, session creation, passkeys, and recovery rotation.
//!
//! The registration, password login, challenge, and passkey login endpoints are unauthenticated and mounted under `/api/auth`.
//! The recovery rotation and passkey management endpoints are protected behind `mac_guard` and `auth_guard` and mounted under `/api/v1/auth`.
//!
//! ## Endpoint Overview
//!
//! | Method | Path | Handler | `PoW` Required | Authenticated |
//! |--------|------|---------|----------------|---------------|
//! | `GET`  | `/api/auth/challenge` | [`challenge::generate()`] | No  | No |
//! | `POST` | `/api/auth/login`    | [`login()`]               | Yes | No |
//! | `POST` | `/api/auth/register` | [`register()`]            | Yes | No |
//! | `POST` | `/api/auth/passkey/login/start` | [`passkey::login_start()`] | No | No |
//! | `POST` | `/api/auth/passkey/login/finish` | [`passkey::login_finish()`] | No | No |
//! | `PUT`  | `/api/v1/auth/recovery` | [`rotate()`]           | No  | Yes |
//! | `POST` | `/api/v1/auth/passkey/register/start` | [`passkey::register_start()`] | No | Yes |
//! | `POST` | `/api/v1/auth/passkey/register/finish` | [`passkey::register_finish()`] | No | Yes |
//! | `GET`  | `/api/v1/auth/passkey` | [`passkey::list()`] | No | Yes |
//! | `DELETE` | `/api/v1/auth/passkey/{credential_id}` | [`passkey::delete()`] | No | Yes |
//!
//! The `pow_guard` middleware enforces the Hashcash Proof-of-Work protocol on
//! the `login` and `register` routes. The `challenge` endpoint is intentionally
//! excluded — it generates the puzzle the client must solve.
//!
//! This module operates on the [`User`], [`Session`], and [`passkey::Record`] domain models.
//!
//! [`User`]: crate::model::user::User
//! [`Session`]: crate::model::session::Session
//! [`passkey::Record`]: crate::model::passkey::Record
/// Stateless Hashcash Proof-of-Work challenge generation.
pub mod challenge;
pub(crate) mod hashcash;
pub use hashcash::HashCash;
mod login;
mod pow_guard;
pub(crate) mod recovery;
mod register;

pub use login::login;
pub use pow_guard::pow_guard;
pub use recovery::rotate;
pub use register::register;

/// `WebAuthn` passkey registration, authentication, listing, and deletion handlers.
pub mod passkey;

#[cfg(test)]
mod test;
