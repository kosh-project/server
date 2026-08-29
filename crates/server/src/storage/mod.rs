/// The storage layer — responsible for all filesystem operations.
///
/// This module manages the Content-Addressable Storage (CAS) vault where
/// all encrypted blobs live on disk. It deliberately knows nothing about
/// users, sessions, or metadata — that is the model layer's concern.
///
/// ## Key responsibilities
///
/// - Accepting a raw byte stream and committing it to disk atomically.
/// - Naming blobs by their BLAKE3 hash (CAS semantics).
/// - Serving blobs back as file handles for streaming to clients.
/// - Deleting blobs when their reference count drops to zero.
///
/// ## Atomicity guarantee
///
/// Every write goes through an internal transaction. The transaction
/// streams bytes to a `<uuid>.tmp` staging file first, then does an atomic
/// `rename(2)` into the vault. If anything fails mid-stream, the temp file
/// is cleaned up and the vault is left untouched.
pub mod error;
pub mod file;
pub mod ledger;
pub mod service;
pub mod transaction;

use std::error::Error as StdErr;

use bytes::Bytes;
use futures::Stream;

pub use error::{Error, Result};
pub use service::Service;

#[cfg(test)]
mod tests;

/// A bundle that ties together a raw byte stream and its declared size.
///
/// This newtype exists to avoid "primitive obsession" — passing `expected_size`
/// and `stream` as separate parameters to multiple layers of functions is error-prone.
/// Bundling them enforces that the two always travel together.
///
/// The `expected_size` is used to pre-allocate disk space via `fallocate` before
/// streaming begins, which improves performance on spinning drives and SD cards by
/// preventing filesystem fragmentation.
pub struct Payload<S, E>
where
    S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
    E: Into<Box<dyn StdErr + Send + Sync>>,
{
    /// The total number of bytes the client claims it will send.
    /// This is trusted for pre-allocation but validated at EOF.
    pub expected_size: u64,
    /// The raw async stream of byte chunks from the request body.
    pub stream: S,
}

impl<S, E> Payload<S, E>
where
    S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
    E: Into<Box<dyn StdErr + Send + Sync>>,
{
    /// Constructs a new `Payload` from an expected size and an async byte stream.
    ///
    /// The `expected_size` accepts any type that can be converted to `u64`,
    /// so passing a `u32` or `usize` directly is fine.
    pub fn new<T>(expected_size: T, stream: S) -> Self
    where
        T: Into<u64>,
    {
        Self {
            expected_size: expected_size.into(),
            stream,
        }
    }
}
