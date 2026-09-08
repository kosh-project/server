//! Delta-CRDT synchronization ledger for per-user encrypted action logs.
//!
//! The ledger is an append-only binary log that acts as the server's "dumb mailbox"
//! for CRDT action events uploaded by Android clients. Each client periodically appends
//! a batch of encrypted actions (moves, deletes, renames) to their ledger. Other devices
//! owned by the same user pull those events and replay them to converge their local state.
//!
//! ## Storage layout
//!
//! Each user gets an isolated directory inside the vault:
//!
//! ```text
//! vault/
//! └── ledgers/
//!     └── <user_id>/
//!         ├── CURRENT           ← UTF-8 file containing the active segment name
//!         ├── delta_0000001     ← old segments (may be pruned)
//!         ├── delta_0000002
//!         └── delta_0000003     ← active segment currently being written
//! ```
//!
//! ## Segment format
//!
//! Every segment file starts with a 500-byte header:
//!
//! | Offset | Size | Field           |
//! |--------|------|-----------------|
//! | 0      | 4    | `b"KOSH"` magic |
//! | 4      | 2    | Format version (`1u16`, LE) |
//! | 6      | 4    | Previous segment ID (`u32`, LE) |
//! | 10     | 490  | Reserved (zeroed) |
//!
//! Payload data follows the header. Each payload is framed by the API layer
//! (`api/sync.rs`) as `[4-byte LE length][payload bytes]`. The server never
//! parses this framing — it is purely a client-side convention.
//!
//! ## Architecture
//!
//! ```text
//! HTTP handler (api/sync.rs)
//!     │
//!     │  mpsc::Sender<Action>
//!     │
//!     ▼
//! Committer (single-writer actor)
//!     ├── Maintains HashMap<user_id, Segment> of open file handles
//!     ├── Appends payload bytes to the active segment
//!     ├── Rotates to a new segment when the active file exceeds 5 MB
//!     └── Deletes old segments on prune requests
//!
//! Handle (Clone-able client)
//!     ├── append()        → sends Action::Append, awaits reply
//!     ├── prune()         → sends Action::Prune, awaits reply
//!     ├── shutdown()      → sends Action::Shutdown, awaits flush confirmation
//!     └── read_segment()  → stateless direct file read (bypasses the actor)
//! ```
//!
//! ## Crash safety
//!
//! The `CURRENT` pointer is updated atomically via a `.tmp` rename. If the server
//! crashes mid-rotation, the next startup detects and discards any ghost file left
//! behind before creating a fresh segment.

mod action;
mod committer;
mod error;
mod handle;
mod segment;

pub use error::Error;
pub type Result<T> = core::result::Result<T, Error>;

pub(crate) use action::AppendReciept;

pub use handle::Handle;

#[cfg(test)]
mod tests;
