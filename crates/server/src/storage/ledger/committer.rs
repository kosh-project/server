use std::{
    collections::{HashMap, hash_map::Entry},
    path::{Path, PathBuf},
};

use bytes::Bytes;
use kosh_core::logger::Module;
use tokio::{
    fs,
    sync::{mpsc::Receiver, oneshot::Sender},
};

use crate::{
    info,
    storage::ledger::{
        AppendReceipt, Namespace, Result,
        action::Action::{self, Append, Prune, Shutdown},
        segment::{ActiveSegment, recovery::SealedSegment},
    },
};

/// The single-writer actor that owns all open segment file handles.
///
/// `Committer` runs inside a dedicated `tokio::spawn` task and processes
/// [`Action`] messages from an `mpsc` bounded channel. Because all writes
/// are serialized through this single task, no `Mutex` is needed on the file
/// handles — Rust's ownership model enforces the single-writer invariant at
/// compile time.
///
/// Users interact with the `Committer` only through the [`Handle`] interface,
/// which sends `Action` variants and awaits results over `oneshot` channels.
///
/// ## In-memory state
///
/// `active_segments` keeps one open [`ActiveSegment`] per `(user_id, namespace)` tuple.
/// The first append causes [`ActiveSegment::load_or_create`] to be called, which either
/// reopens `current.log` or creates a fresh one. Subsequent appends reuse the cached
/// handle with zero disk seek or directory scans.
///
/// [`Handle`]: crate::storage::ledger::handle::Handle
/// [`Action`]: crate::storage::ledger::action::Action
pub struct Committer {
    /// Absolute path to the vault root directory.
    vault_path: PathBuf,
    /// The receiving end of the `mpsc` channel that carries incoming [`Action`]s.
    receiver: Receiver<Action>,
    /// In-memory cache of open active segment handles, keyed by `(user_id, namespace)`.
    active_segments: HashMap<(i64, Namespace), ActiveSegment>,
}

impl Committer {
    /// Creates a new `Committer` bound to the given vault path and channel receiver.
    ///
    /// This only initializes the struct. Call [`run`] to start processing messages.
    ///
    /// [`run`]: Self::run
    pub fn new<P>(vault_path: P, rx: Receiver<Action>) -> Self
    where
        P: AsRef<Path>,
    {
        Self {
            vault_path: vault_path.as_ref().to_owned(),
            active_segments: HashMap::with_capacity(100),
            receiver: rx,
        }
    }

    /// Starts the actor event loop.
    ///
    /// Each action is handled to completion before the next one is dequeued,
    /// ensuring strictly sequential writes within a user's ledger partition.
    pub(crate) async fn run(mut self) {
        while let Some(action) = self.receiver.recv().await {
            match action {
                Append {
                    user_id,
                    namespace,
                    payload,
                    reply,
                } => {
                    let result = self.append(user_id, namespace, payload).await;
                    let _ = reply.send(result);
                }
                Prune {
                    user_id,
                    namespace,
                    safe_prune_seq,
                    reply,
                } => {
                    let result =
                        self.prune(user_id, namespace, safe_prune_seq).await;
                    let _ = reply.send(result);
                }
                Shutdown { reply } => {
                    self.shutdown(reply).await;
                    break;
                }
            }
        }
    }

    /// Deletes sealed historical segments whose start sequence is strictly <= `safe_prune_seq`.
    ///
    /// Returns the number of segment files safely removed from disk.
    async fn prune(
        &self,
        user_id: i64,
        namespace: Namespace,
        safe_prune_seq: u64,
    ) -> Result<usize> {
        let dir = self
            .vault_path
            .join("ledgers")
            .join(user_id.to_string())
            .join(namespace.as_str());

        let sealed_segments = SealedSegment::list(&dir).await?;
        let mut pruned_count = 0;

        for sealed in sealed_segments {
            if sealed.start_seq <= safe_prune_seq {
                fs::remove_file(&sealed.path).await?;
                pruned_count += 1;
            }
        }

        info!(
            Module::Ledger,
            "Pruned {pruned_count} sealed segments for user {user_id} namespace {namespace} up to seq {safe_prune_seq}"
        );

        Ok(pruned_count)
    }

    /// Appends a raw payload to the active segment for a user and namespace.
    ///
    /// Encapsulates automatic rotation and Big-Endian wire framing.
    async fn append(
        &mut self,
        user_id: i64,
        namespace: Namespace,
        payload: Bytes,
    ) -> Result<AppendReceipt> {
        let dir = self
            .vault_path
            .join("ledgers")
            .join(user_id.to_string())
            .join(namespace.as_str());

        let active = match self.active_segments.entry((user_id, namespace)) {
            Entry::Occupied(segment) => segment.into_mut(),
            Entry::Vacant(entry) => {
                let segment = ActiveSegment::load_or_create(&dir).await?;
                entry.insert(segment)
            }
        };

        if active.should_rotate() {
            active.rotate().await?;

            info!(
                Module::Ledger,
                "Rotated ledger for user {user_id} namespace {namespace} to fresh segment"
            );
        }

        active.append(&payload).await
    }

    /// Flushes all open segment files to disk and notifies the caller.
    async fn shutdown(mut self, reply: Sender<()>) {
        for segment in self.active_segments.values_mut() {
            let _ = segment.sync().await;
        }

        let _ = reply.send(());
    }
}
