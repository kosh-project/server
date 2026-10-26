//! `WebAuthn` (FIDO2) passkey authentication and credential management.
//!
//! Passkeys enable hardware-backed, phishing-resistant, passwordless authentication using
//! platform authenticators (e.g., Apple Touch ID / Face ID, Windows Hello, Android Biometrics)
//! and roaming security keys (e.g., YubiKeys).
//!
//! ## Endpoint Overview
//!
//! | Method | Path | Handler | Description | Authenticated |
//! |--------|------|---------|-------------|---------------|
//! | `POST` | `/api/auth/passkey/login/start` | [`crate::api::auth::passkey::login_start`] | Initiates discoverable passkey login ceremony | No |
//! | `POST` | `/api/auth/passkey/login/finish` | [`crate::api::auth::passkey::login_finish`] | Verifies authenticator assertion and issues session token | No |
//! | `POST` | `/api/v1/auth/passkey/register/start` | [`crate::api::auth::passkey::register_start`] | Generates passkey creation challenge for caller | Yes |
//! | `POST` | `/api/v1/auth/passkey/register/finish` | [`crate::api::auth::passkey::register_finish`] | Verifies attestation and saves passkey credential | Yes |
//! | `GET`  | `/api/v1/auth/passkey` | [`crate::api::auth::passkey::list`] | Returns metadata for all passkeys owned by caller | Yes |
//! | `DELETE` | `/api/v1/auth/passkey/{credential_id}` | [`crate::api::auth::passkey::delete`] | Deletes a registered passkey owned by caller | Yes |
//!
//! ## Mathematical Bijection between SQLite User ID and `WebAuthn` Handle
//!
//! The `WebAuthn` specification requires that authenticators store an opaque user handle
//! (`user.id`) representing the account. This handle must be stable and unique.
//!
//! In this server, `SQLite` user primary keys are positive 63-bit integers (`i64` in the range
//! `0..=i64::MAX`). To avoid maintaining an auxiliary stateful mapping table in the database,
//! `user_id_to_handle` and `handle_to_user_id` establish a lossless, deterministic,
//! zero-allocation mathematical bijection between `i64` and [`uuid::Uuid`]:
//!
//! - Forward mapping: `i64 -> u64 -> u128 -> Uuid::from_u128`
//! - Reverse mapping: `Uuid -> Uuid::as_u128 -> i64::try_from`
//!
//! Any valid `SQLite` user ID maps to a unique `Uuid`, and deserializing that `Uuid` reproduces
//! the exact original `i64`. If an out-of-bounds `u128` is received in a client assertion,
//! the conversion safely rejects it.

use uuid::Uuid;

use crate::api::{self, Error::BadRequest};

/// Converts an internal `SQLite` user ID (`i64`) into a `WebAuthn` user handle [`uuid::Uuid`].
///
/// This implements the forward direction of the lossless mathematical bijection between
/// relational database primary keys and `WebAuthn` user handles. The 64-bit non-negative integer
/// is widened into a 128-bit unsigned integer and wrapped as a [`uuid::Uuid`].
///
/// # Errors
///
/// - Returns [`crate::api::Error::IntegerConversion`] if `user_id` is negative.
pub(crate) fn user_id_to_handle(user_id: i64) -> api::Result<Uuid> {
    let raw = u64::try_from(user_id)?;

    Ok(Uuid::from_u128(u128::from(raw)))
}

/// Converts a `WebAuthn` user handle [`uuid::Uuid`] back into an internal `SQLite` user ID (`i64`).
///
/// This implements the reverse direction of the lossless mathematical bijection. It extracts
/// the underlying 128-bit integer from the [`uuid::Uuid`] and downcasts it to `i64`.
///
/// # Errors
///
/// - Returns [`crate::api::Error::BadRequest`] if the handle's 128-bit value exceeds the positive range of `i64`.
pub(crate) fn handle_to_user_id(handle: &Uuid) -> api::Result<i64> {
    let raw = handle.as_u128();

    i64::try_from(raw)
        .map_err(|_| BadRequest("Invalid user handle integer bounds"))
}

mod delete;
mod list;
mod login;
mod register;

pub use delete::delete;
pub use list::list;
pub use login::{login_finish, login_start};
pub use register::{register_finish, register_start};

#[cfg(test)]
mod test;
