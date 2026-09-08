use crate::storage::{
    Payload,
    ledger::{
        AppendReciept,
        Error::{self, CommitterDead},
        Result,
    },
};
use std::{
    cmp,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use bytes::Bytes;
use tokio::{
    fs::File,
    io::AsyncSeekExt,
    sync::{
        mpsc::{self, Sender},
        oneshot,
    },
};

use crate::storage::ledger::{action::Action, committer::Committer};

/// A cheap, cloneable client handle to the ledger actor.
///
/// `Handle` is the public interface for all ledger operations. It wraps an
/// `mpsc::Sender<Action>` and provides typed async methods that send requests
/// to the single-writer [`Committer`] task and await their results.
///
/// Because `Handle` implements `Clone`, it can be stored in [`AppState`] and
/// cloned freely on every incoming HTTP request without copying any data — only
/// the channel sender's reference count is incremented.
///
/// ## Read vs. write paths
///
/// - **Write path** (`append`, `prune`, `shutdown`): requests are sent through
///   the `mpsc` channel to the [`Committer`] actor, which processes them
///   serially. This guarantees exclusive access to the segment files.
/// - **Read path** (`read_segment`): opens a segment file directly from the
///   calling task without going through the channel. This enables many
///   concurrent readers at zero cost to the actor queue.
///
/// [`Committer`]: crate::storage::ledger::committer::Committer
/// [`AppState`]: crate::app::State
#[derive(Clone)]
pub struct Handle {
    /// The sender half of the bounded `mpsc` channel that drives the actor.
    tx: Sender<Action>,
}

impl Handle {
    /// Spawns the [`Committer`] actor and returns a `Handle` connected to it.
    ///
    /// Creates a bounded `mpsc` channel with a capacity of 100, constructs a
    /// [`Committer`] bound to `vault_dir`, and spawns its `run` loop as a
    /// detached Tokio task. The `Handle` wraps the sender end of that channel.
    ///
    /// This is the only way to create a `Handle`. It should be called once at
    /// server startup and the result stored in [`AppState`].
    ///
    /// [`Committer`]: crate::storage::ledger::committer::Committer
    /// [`AppState`]: crate::app::State
    #[must_use]
    pub fn spawn(vault_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let committer = Committer::new(vault_dir, rx);
        tokio::spawn(committer.run());
        Self { tx }
    }

    /// Returns a reference to the raw `mpsc::Sender` used to communicate with
    /// the actor.
    ///
    /// This is exposed for use in `main.rs`, where the sender must be cloned
    /// before `AppState` is moved into the Axum router so that a reference to
    /// it remains available for the graceful shutdown sequence.
    #[must_use]
    pub const fn sender(&self) -> &Sender<Action> {
        &self.tx
    }

    /// Appends a pre-framed payload to the active segment for a user.
    ///
    /// Sends an [`Action::Append`] to the [`Committer`] actor and blocks until
    /// the result is returned over a `oneshot` channel.
    ///
    /// The caller is responsible for any framing applied before calling this
    /// method. The API handler (`api/sync.rs`) prepends a 4-byte little-endian
    /// length prefix so that the Android client knows the boundaries of each
    /// encrypted action when streaming back the segment.
    ///
    /// ## Errors
    ///
    /// Returns [`Error::CommitterDead`] if the actor task has terminated
    /// unexpectedly, either because the channel is closed or the `oneshot`
    /// reply was dropped.
    ///
    /// [`Action::Append`]: crate::storage::ledger::action::Action::Append
    /// [`Committer`]: crate::storage::ledger::committer::Committer
    pub async fn append(
        &self,
        user_id: i64,
        payload: Bytes,
    ) -> Result<AppendReciept> {
        let (reply, recv) = oneshot::channel();

        let action = Action::Append {
            user_id,
            payload,
            reply,
        };
        self.tx.send(action).await.map_err(|_| CommitterDead)?;

        recv.await.map_err(|_| CommitterDead)?
    }
}

impl Handle {
    /// Opens a segment file and returns a seeked, readable file handle.
    ///
    /// This method **does not** go through the actor channel. It opens the file
    /// directly from the calling task, which allows many concurrent reads without
    /// blocking the writer. Each caller receives its own independent file
    /// descriptor seeked to the requested position.
    ///
    /// ## Security
    ///
    /// Before constructing the path, the `file_name` argument is validated:
    /// - Must start with the prefix `"delta_"`.
    /// - Must not contain `'/'` or `'\\'` to prevent path traversal attacks.
    ///
    /// If validation fails, [`Error::InvalidFileName`] is returned immediately
    /// without touching the filesystem.
    ///
    /// ## Offset floor
    ///
    /// The requested `offset` is silently clamped to a minimum of `500` bytes.
    /// This prevents any caller from reading bytes `0..500`, which contain the
    /// `KOSH` binary header and are not meaningful to clients.
    ///
    /// ## Errors
    ///
    /// | Condition | Error returned |
    /// |-----------|----------------|
    /// | `file_name` fails validation | [`Error::InvalidFileName`] |
    /// | File does not exist on disk | [`Error::SegmentNotFound`] |
    /// | `offset` (after clamping) exceeds the file length | [`Error::InvalidOffset`] |
    /// | Any other IO failure | [`Error::IoError`] |
    pub async fn read_segment<P>(
        &self,
        vault_path: P,
        user_id: i64,
        file_name: &str,
        offset: u64,
    ) -> Result<File>
    where
        P: AsRef<Path>,
    {
        if file_name.contains('\\')
            || file_name.contains('/')
            || !file_name.starts_with("delta_")
        {
            return Err(Error::InvalidFileName);
        }

        let path = vault_path
            .as_ref()
            .join("ledgers")
            .join(user_id.to_string())
            .join(file_name);

        let mut file = File::open(&path).await.map_err(|e| match e.kind() {
            ErrorKind::NotFound => Error::SegmentNotFound,
            _ => Error::IoError(e),
        })?;

        let safe_offset = cmp::max(offset, 500);

        let metadata = file.metadata().await.map_err(Error::IoError)?;

        if safe_offset > metadata.len() {
            return Err(Error::InvalidOffset);
        }

        file.seek(std::io::SeekFrom::Start(safe_offset))
            .await
            .map_err(Error::IoError)?;

        Ok(file)
    }

    /// Sends a shutdown signal to the actor and waits for all data to be flushed.
    ///
    /// This is a **static method** that takes the raw `Sender` rather than
    /// `&self`. This design is intentional: in `main.rs`, the `Handle` is moved
    /// into the Axum `AppState` which is then moved into the router. The raw
    /// sender is cloned before that move so that the shutdown path still has
    /// something to send on.
    ///
    /// Sends [`Action::Shutdown`] and awaits the reply. The [`Committer`] calls
    /// `sync_all()` on every open segment before replying, guaranteeing all
    /// page cache data is committed to disk.
    ///
    /// If the channel is already closed (i.e. the actor crashed), this method
    /// silently does nothing rather than panicking.
    ///
    /// [`Action::Shutdown`]: crate::storage::ledger::action::Action::Shutdown
    /// [`Committer`]: crate::storage::ledger::committer::Committer
    pub async fn shutdown(sender: &Sender<Action>) {
        let (tx, rx) = oneshot::channel();
        if sender.send(Action::Shutdown { reply: tx }).await.is_ok() {
            let _ = rx.await;
        }
    }

    /// Deletes old delta segments for a user by forwarding a prune request to
    /// the actor.
    ///
    /// All segments with a numeric ID strictly less than `before` are removed
    /// from disk, provided the actor's safety checks pass. See
    /// [`Committer::prune`] for the full deletion algorithm and invariants.
    ///
    /// ## Errors
    ///
    /// Returns [`Error::InvalidPrune`] if `before > active_id`, indicating that
    /// the caller is trying to prune an active or non-existent segment.
    ///
    /// Returns [`Error::CommitterDead`] if the actor task has terminated
    /// unexpectedly.
    ///
    /// [`Committer::prune`]: crate::storage::ledger::committer::Committer::prune
    pub async fn prune(&self, user_id: i64, before: u32) -> Result<()> {
        let (reply, recv) = oneshot::channel();

        self.tx
            .send(Action::Prune {
                user_id,
                before,
                reply,
            })
            .await
            .map_err(|_| Error::CommitterDead)?;

        recv.await.map_err(|_| Error::CommitterDead)?
    }
}
