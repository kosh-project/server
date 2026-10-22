use bytes::Bytes;
use tokio::sync::oneshot::Sender;

use crate::storage::ledger::{Namespace, Result};

/// Commands that can be sent to the [`crate::storage::ledger::committer::Committer`] actor.
///
/// The `Committer` is a single-writer background task that owns all open file
/// handles. Every mutation to the ledger — appending a payload, pruning old
/// segments, or shutting down — is expressed as an `Action` sent over an
/// `mpsc` channel.
///
/// Each variant carries a `reply` field: a [`tokio::sync::oneshot::Sender`]
/// the caller uses to await the result. This gives the caller an async
/// request-response primitive without sharing any mutable state.
#[derive(Debug)]
pub enum Action {
    /// Write an encrypted payload to the active delta segment for a user.
    ///
    /// The `Committer` prepends nothing to the bytes — framing (e.g. the
    /// 4-byte little-endian length prefix) must be applied by the caller
    /// before constructing this action. On success, the reply carries an
    /// [`AppendReceipt`] containing the segment name and the new high-water
    /// mark offset.
    Append {
        /// The database primary key of the user whose ledger receives this payload.
        user_id: i64,
        /// The raw bytes to append to the active segment file.
        payload: Bytes,
        /// Channel used to return the result to the caller.
        reply: Sender<Result<AppendReceipt>>,

        namespace: Namespace,
    },

    /// Delete all delta segments strictly older than `before` for a user.
    ///
    /// Segments are identified by their numeric suffix (e.g. `delta_0000003`
    /// has ID `3`). Any segment whose ID is less than `before` is removed from
    /// disk, provided it is not the currently active segment.
    ///
    /// The `Committer` rejects `before > active_id` with
    /// [`crate::storage::ledger::Error::InvalidPrune`] to prevent a client
    /// from accidentally deleting history that has not yet been fully
    /// acknowledged.
    Prune {
        /// The database primary key of the user whose old segments are deleted.
        user_id: i64,
        /// The namespace to prune.
        namespace: Namespace,
        /// Highest sequence acknowledged by all active devices and baseline snapshots
        safe_prune_seq: u64,
        /// Channel used to return the result to the caller.
        reply: Sender<Result<usize>>,
    },

    /// Signal the `Committer` to flush all open file handles and exit.
    ///
    /// The actor calls [`tokio::fs::File::sync_all`] on every open segment
    /// before sending the reply, guaranteeing that OS-level page cache data
    /// is committed to disk before the process exits.
    Shutdown {
        /// Channel used to notify the caller that all data has been flushed.
        reply: Sender<()>,
    },
}

/// Confirmation returned to the caller after a successful [`Action::Append`].
///
/// The two fields together form the **high-water mark cursor** that the Android
/// client stores locally after each sync. On the next sync session, the client
/// sends `(file_name, offset)` back to the server as query parameters on the
/// `GET /api/v1/sync/delta` endpoint to resume streaming from exactly where it
/// left off.
#[derive(Debug, Clone)]
pub struct AppendReceipt {
    pub sequence_id: u64,
    pub file_name: String,
    pub offset: u64,
}
