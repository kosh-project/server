//! Ephemeral ECDH key exchange mailbox endpoints for device pairing.
//!
//! This module provides HTTP route handlers that implement an ephemeral blind relay
//! for out-of-band Elliptic Curve Diffie-Hellman (ECDH) key exchange between devices
//! belonging to the same authenticated user account.
//!
//! # Protocol Overview
//!
//! When pairing a new device (e.g. mobile client) with an existing authenticated device
//! (e.g. desktop client), the two endpoints establish end-to-end encrypted device keys
//! without exposing secrets or key material to the server:
//!
//! 1. **Allocation (`POST /api/v1/devices/mailbox`):**
//!    The receiving device invokes [`create`](crate::api::mailbox::create) to allocate a temporary mailbox. The server
//!    generates an 8-character lowercase Crockford Base32 identifier and returns it
//!    together with the expiration countdown in seconds (`expires_in: 300`).
//! 2. **Out-of-band Transfer:**
//!    The receiving device presents the 8-character code (or a QR code representing it)
//!    to the user. The sending device scans or enters the code out-of-band.
//! 3. **Fulfillment (`POST /api/v1/devices/mailbox/{id}/fulfill`):**
//!    The sending device generates its ECDH public key or key agreement payload and
//!    submits it as a hex string to [`fulfill`](crate::api::mailbox::fulfill). The payload must be between 120 and
//!    8,192 hex characters (60 to 4,096 raw bytes). The server stores the raw bytes
//!    in memory and marks the mailbox as fulfilled.
//! 4. **Pickup & Burn (`GET /api/v1/devices/mailbox/{id}`):**
//!    The receiving device polls [`pickup`](crate::api::mailbox::pickup). While pending, the server responds with
//!    `{"status": "pending"}`. Once fulfilled, it returns the hex-encoded payload and
//!    immediately deletes the mailbox from memory, guaranteeing single-use
//!    "burn-after-reading" semantics.
//!
//! # Security Contract
//!
//! - **Untrusted Blind Relay:** The server never inspects or decrypts the exchanged payload;
//!   all cryptographic verification happens client-side between the pairing devices.
//! - **Mutual Authentication:** All mailbox endpoints require authentication via [`auth_guard`]
//!   and [`mac_guard`].
//! - **Cross-Tenant Isolation & Anti-Enumeration:** Mailboxes are strictly bound to the
//!   authenticated caller's [`UserId`]. Access attempts from a different account return
//!   HTTP `404 Not Found`, denying attackers an oracle to probe or enumerate active mailbox IDs.
//! - **Bounded Lifetime:** Mailboxes are stored in an in-memory [`moka::future::Cache`]
//!   with a hard 300-second Time-To-Live ([`Mailbox::TTL_SECS`]). Expired mailboxes are
//!   evicted automatically.
//!
//! [`auth_guard`]: crate::api::middleware::auth_guard
//! [`mac_guard`]: crate::api::middleware::mac_guard
//! [`UserId`]: crate::app::state::UserId
//! [`Mailbox::TTL_SECS`]: crate::app::Mailbox::TTL_SECS

mod create;
mod fulfill;
mod pickup;

pub use create::create;
pub use fulfill::fulfill;
pub use pickup::pickup;

#[cfg(test)]
mod test;
