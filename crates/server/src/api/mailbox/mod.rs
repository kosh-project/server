//! Ephemeral ECDH key exchange mailbox endpoints for device pairing.
//!
//! This module provides HTTP route handlers that implement an in-memory blind relay
//! for out-of-band Elliptic Curve Diffie-Hellman (ECDH) key agreement and device
//! pairing between an existing authenticated device and a new unauthenticated client.
//!
//! # Protocol UX Flows
//!
//! Device pairing supports two bidirectional handshake flows depending on which device
//! displays or scans the pairing code:
//!
//! ## Flow A: Receiver Displays QR Code (e.g., Mobile Phone to Desktop)
//!
//! 1. **Open Mailbox (`POST /api/auth/mailbox/open`):**
//!    The unauthenticated pairing device generates an ephemeral X25519 keypair and submits
//!    its public key (`pub_temp`, 32 bytes / 64 hex chars) under Hashcash Proof-of-Work
//!    ([`pow_guard`]). The server allocates an 8-character Crockford Base32 mailbox identifier
//!    and a cryptographically secure 32-byte polling token (`polling_token`), initializing
//!    the state to `Ready`.
//! 2. **Out-of-band Display:**
//!    The pairing device presents the mailbox identifier (or a QR code containing it)
//!    to the user.
//! 3. **Mint Registration Ticket (`POST /api/v1/devices/link-ticket`):**
//!    The authenticated device scans the code, authenticates via session token ([`auth_guard`],
//!    [`mac_guard`]), and requests an ephemeral registration ticket (`registration_ticket`,
//!    valid for 300 seconds).
//! 4. **Fulfillment (`POST /api/v1/devices/mailbox/{id}/fulfill`):**
//!    The authenticated device derives the shared ECDH key agreement secret, encrypts
//!    the registration ticket and initial vault envelope keys, and deposits the ciphertext
//!    (120 to 8,192 hex characters). The mailbox transitions to `Fulfilled`.
//! 5. **Pickup & Burn (`GET /api/auth/mailbox/{id}/pickup`):**
//!    The pairing device polls the pickup endpoint using `Authorization: Bearer <polling_token>`.
//!    Once fulfilled, the server returns the ciphertext and immediately burns the entry
//!    from RAM. The pairing device decrypts the payload and completes passkey registration
//!    using the registration ticket.
//!
//! ## Flow B: Sender Displays QR Code (e.g., Desktop to Mobile Phone)
//!
//! 1. **Offer Mailbox (`POST /api/v1/devices/mailbox`):**
//!    The authenticated device allocates a pairing mailbox bound to its `user_id`. The server
//!    returns the 8-character mailbox identifier with state `Waiting`.
//! 2. **Poll Status (`GET /api/v1/devices/mailbox/{id}/status`):**
//!    The authenticated device polls the status endpoint while displaying the pairing QR code.
//!    While awaiting the receiver, it receives `{"status": "waiting"}`.
//! 3. **Join Mailbox (`POST /api/auth/mailbox/{id}/join`):**
//!    The unauthenticated pairing device scans the code and submits its ephemeral public key
//!    under Hashcash Proof-of-Work. The server deposits the key, generates a polling token,
//!    and transitions the state to `Ready`.
//! 4. **Status Ready & Fulfillment:**
//!    The authenticated device's next status poll returns `{"status": "ready", "pub_temp": "<hex>"}`.
//!    It derives the shared secret, encrypts the pairing payload, and submits it to `fulfill`.
//! 5. **Pickup & Burn (`GET /api/auth/mailbox/{id}/pickup`):**
//!    The pairing device polls with its token, receives the ciphertext, and the mailbox is burned.
//!
//! # Security Invariants
//!
//! - **RAM-Only Ephemeral Storage:** Mailboxes exist exclusively in an in-memory
//!   [`moka::future::Cache`] with a hard 300-second Time-To-Live ([`Mailbox::TTL_SECS`])
//!   and a 1,000-entry capacity limit. No mailbox state is ever persisted to SQLite.
//! - **Untrusted Blind Relay:** The server never inspects or decrypts the exchanged ciphertext;
//!   all cryptographic verification happens end-to-end between client devices.
//! - **Cross-Tenant Isolation & Anti-Enumeration:** Mailboxes created via Flow B are strictly
//!   bound to the owner's [`UserId`]. In both `Waiting` and `Ready` states, access attempts from
//!   different accounts return HTTP `404 Not Found`, denying attackers an enumeration oracle.
//! - **Single-Use Burn-After-Reading:** A fulfilled mailbox is evicted immediately upon
//!   first successful retrieval by the holder of the polling token.
//!
//! [`auth_guard`]: crate::api::middleware::auth_guard
//! [`mac_guard`]: crate::api::middleware::mac_guard
//! [`pow_guard`]: crate::api::auth::pow_guard
//! [`UserId`]: crate::app::state::UserId
//! [`Mailbox::TTL_SECS`]: crate::app::Mailbox::TTL_SECS

mod create;
mod fulfill;
mod join;
mod open;
mod pickup;
mod status;
mod ticket;

pub use create::create;
pub use fulfill::fulfill;
pub use join::join;
pub use open::open;
pub use pickup::pickup;
pub use status::status;
pub use ticket::ticket;

#[cfg(test)]
mod test;
