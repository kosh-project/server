use crate::{
    info,
    storage::ledger::{
        AppendReceipt,
        Error::{self, CommitterDead},
        Namespace, Result,
    },
};
use std::{
    cmp,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use bytes::Bytes;
use kosh_core::logger::Module;
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
/// to the single-writer `Committer` task and await their results.
///
/// Because `Handle` implements `Clone`, it can be stored in [`AppState`] and
/// cloned freely on every incoming HTTP request without copying any data — only
/// the channel sender's reference count is incremented.
///
/// ## Read vs. write paths
///
/// - **Write path** (`append`, `prune`, `shutdown`): requests are sent through
///   the `mpsc` channel to the `Committer` actor, which processes them
///   serially. This guarantees exclusive access to the segment files.
/// - **Read path** (`read_segment`): opens a segment file directly from the
///   calling task without going through the channel. This enables many
///   concurrent readers at zero cost to the actor queue.
///
/// [`AppState`]: crate::app::State
#[derive(Clone)]
pub struct Handle {
    /// The sender half of the bounded `mpsc` channel that drives the actor.
    tx: Sender<Action>,
}

impl Handle {
    /// Spawns the `Committer` actor and returns a `Handle` connected to it.
    ///
    /// Creates a bounded `mpsc` channel with a capacity of 100, constructs a
    /// `Committer` bound to `vault_dir`, and spawns its `run` loop as a
    /// detached Tokio task. The `Handle` wraps the sender end of that channel.
    #[must_use]
    pub fn spawn(vault_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel(100);
        let committer = Committer::new(vault_dir, rx);
        tokio::spawn(committer.run());

        info!(Module::Ledger, "Ledger committer actor spawned");
        Self { tx }
    }

    /// Returns a reference to the raw `mpsc::Sender` used to communicate with
    /// the actor.
    #[must_use]
    pub const fn sender(&self) -> &Sender<Action> {
        &self.tx
    }

    /// Appends a raw CRDT payload to the active segment for a user and namespace.
    ///
    /// Sends an `Action::Append` to the `Committer` actor and blocks until
    /// the result is returned over a `oneshot` channel.
    ///
    /// ## Errors
    ///
    /// Returns [`Error::CommitterDead`] if the actor task has terminated
    /// unexpectedly.
    pub async fn append(
        &self,
        user_id: i64,
        namespace: Namespace,
        payload: Bytes,
    ) -> Result<AppendReceipt> {
        let (reply, recv) = oneshot::channel();

        let action = Action::Append {
            user_id,
            namespace,
            payload,
            reply,
        };
        self.tx.send(action).await.map_err(|_| CommitterDead)?;

        recv.await.map_err(|_| CommitterDead)?
    }
}

impl Handle {
    /// Opens a segment file directly for reading, seeked to the requested position.
    ///
    /// This method **does not** go through the actor channel. It opens the file
    /// directly from the calling task, allowing concurrent readers without
    /// blocking writes.
    ///
    /// ## Security
    ///
    /// Rejects filenames with path traversal characters or filenames that do not
    /// end with `".log"`.
    ///
    /// ## Offset floor
    ///
    /// The requested `offset` is silently clamped to a minimum of `500` bytes (the KOSH header).
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidFileName`] if the file name contains traversal characters or
    /// does not have a `.log` extension, [`Error::SegmentNotFound`] if the file does not exist,
    /// [`Error::InvalidOffset`] if the offset exceeds the file length, or [`Error::IoError`] on I/O failure.
    pub async fn read_segment<P>(
        &self,
        vault_path: P,
        user_id: i64,
        namespace: Namespace,
        file_name: &str,
        offset: u64,
    ) -> Result<File>
    where
        P: AsRef<Path>,
    {
        if file_name.is_empty()
            || file_name.contains("..")
            || file_name.contains('/')
            || file_name.contains('\\')
            || !file_name.chars().all(|c| {
                c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
            })
            || Path::new(file_name).file_name().and_then(|f| f.to_str())
                != Some(file_name)
            || !Path::new(file_name)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("log"))
        {
            return Err(Error::InvalidFileName);
        }

        let path = vault_path
            .as_ref()
            .join("ledgers")
            .join(user_id.to_string())
            .join(namespace.as_str())
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
    pub async fn shutdown(sender: &Sender<Action>) {
        let (tx, rx) = oneshot::channel();
        if sender.send(Action::Shutdown { reply: tx }).await.is_ok() {
            let _ = rx.await;
        }
    }

    /// Deletes sealed delta segments whose start sequence <= `safe_prune_seq`.
    ///
    /// Returns the number of segment files removed.
    ///
    /// # Errors
    ///
    /// Returns [`Error::CommitterDead`] if the committer actor has stopped,
    /// or an error returned by the committer task.
    pub async fn prune(
        &self,
        user_id: i64,
        namespace: Namespace,
        safe_prune_seq: u64,
    ) -> Result<usize> {
        let (reply, recv) = oneshot::channel();

        self.tx
            .send(Action::Prune {
                user_id,
                namespace,
                safe_prune_seq,
                reply,
            })
            .await
            .map_err(|_| Error::CommitterDead)?;

        recv.await.map_err(|_| Error::CommitterDead)?
    }
}
