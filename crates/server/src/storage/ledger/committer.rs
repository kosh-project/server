use std::{
    collections::{HashMap, hash_map::Entry},
    io::ErrorKind,
    path::{Path, PathBuf},
};

use bytes::Bytes;
use tokio::{
    fs::{self},
    io::AsyncWriteExt,
    sync::{mpsc::Receiver, oneshot::Sender},
};

use crate::storage::ledger::{
    AppendReciept, Error, Result,
    action::Action::{self, Append, Prune, Shutdown},
    segment::Segment,
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
/// `active_users` keeps one open [`Segment`] per user ID. The first append for
/// a given user causes [`Segment::load_or_create`] to be called, which either
/// reopens the segment named in the `CURRENT` file or creates a fresh one.
/// Subsequent appends from the same user reuse the cached handle with no
/// additional disk I/O.
///
/// [`Handle`]: crate::storage::ledger::handle::Handle
/// [`Action`]: crate::storage::ledger::action::Action
pub struct Committer {
    /// Absolute path to the vault root directory.
    vault_path: PathBuf,
    /// The receiving end of the `mpsc` channel that carries incoming [`Action`]s.
    ///
    /// [`Action`]: crate::storage::ledger::action::Action
    receiver: Receiver<Action>,
    /// In-memory cache of open segment handles, keyed by user ID.
    ///
    /// Pre-allocated with capacity 100 to avoid early rehashing on a server
    /// that has many active users.
    active_users: HashMap<i64, Segment>,
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
            active_users: HashMap::with_capacity(100),
            receiver: rx,
        }
    }

    /// Starts the actor event loop.
    ///
    /// This method drives the actor until the channel is closed or an
    /// [`Action::Shutdown`] message is received. It is intended to be passed
    /// directly to `tokio::spawn`:
    ///
    /// ```ignore
    /// tokio::spawn(committer.run());
    /// ```
    ///
    /// Each action is handled to completion before the next one is dequeued,
    /// ensuring strictly sequential writes within a user's ledger and between
    /// different users' ledgers.
    pub(crate) async fn run(mut self) {
        while let Some(action) = self.receiver.recv().await {
            match action {
                Append {
                    user_id,
                    payload,
                    reply,
                } => {
                    let result = self.append(user_id, payload).await;
                    let _ = reply.send(result);
                }
                Prune {
                    user_id,
                    before,
                    reply,
                } => {
                    let result = self.prune(user_id, before).await;
                    let _ = reply.send(result);
                }
                Shutdown { reply } => {
                    self.shutdown(reply).await;
                    break;
                }
            }
        }
    }

    /// Deletes old delta segments for a user, subject to safety constraints.
    ///
    /// ## Algorithm
    ///
    /// 1. Fetch the numeric ID of the currently active segment.
    ///    - If the user has no ledger at all, return `Ok(())` immediately.
    /// 2. Reject the request if `before > active_id` — the caller is trying to
    ///    prune a segment that is currently active or does not exist yet.
    /// 3. Scan the user's ledger directory. For each file whose name matches
    ///    `delta_<id>` where `id < before`:
    ///    - Double-check that the file is not the active segment (second safety net).
    ///    - Remove the file from disk.
    ///
    /// ## Error handling
    ///
    /// - A `NotFound` error on `fs::read_dir` means the directory was deleted
    ///   by something external; the operation is treated as a no-op.
    /// - Any other IO error from `read_dir` is bubbled up as [`Error::IoError`].
    /// - IO errors from individual `fs::remove_file` calls are propagated
    ///   immediately, aborting the prune mid-way.
    async fn prune(&self, user_id: i64, before: u32) -> Result<()> {
        let dir = self.vault_path.join("ledgers").join(user_id.to_string());

        let Some(active_id) = self.active_segment_id(&dir, user_id).await?
        else {
            return Ok(());
        };

        if before > active_id {
            return Err(Error::InvalidPrune);
        }

        let mut entries = match fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(Error::IoError(e)),
        };

        let active_segment = self
            .active_users
            .get(&user_id)
            .map(|s| s.file_name.as_str());

        while let Some(entry) = entries.next_entry().await? {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };

            if file_name.starts_with("delta_") {
                let id_str = file_name.trim_start_matches("delta_");
                if let Ok(id) = id_str.parse::<u32>()
                    && id < before
                    && Some(file_name) != active_segment
                {
                    fs::remove_file(entry.path()).await?;
                }
            }
        }

        Ok(())
    }

    /// Resolves the numeric ID of a user's current active segment.
    ///
    /// The resolution order is:
    /// 1. If the user has an entry in `active_users`, parse the ID from the
    ///    cached segment's filename. This is the fast path and avoids disk I/O.
    /// 2. Otherwise, read the `CURRENT` pointer file from disk and parse the ID.
    ///    - `NotFound` means no ledger exists yet; return `None`.
    ///    - Any other IO error is returned as [`Error::IoError`].
    ///
    /// Returns `None` when the user has no ledger, allowing the caller to skip
    /// prune operations on users with no history.
    async fn active_segment_id(
        &self,
        dir: &Path,
        user_id: i64,
    ) -> Result<Option<u32>> {
        if let Some(segment) = self.active_users.get(&user_id) {
            return Ok(parse_segment_id(&segment.file_name));
        }

        match fs::read_to_string(dir.join("CURRENT")).await {
            Ok(current) => Ok(parse_segment_id(current.trim())),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Error::IoError(e)),
        }
    }

    /// Appends a raw payload to the active segment for a user.
    ///
    /// ## Segment lifecycle
    ///
    /// - If no open segment exists for `user_id`, one is opened or created via
    ///   [`Segment::load_or_create`] and cached in `active_users`.
    /// - If the active segment is at or above the 5 MB size threshold, it is
    ///   rotated before the write occurs. The rotation is atomic: the new file
    ///   and its `CURRENT` pointer are durable before this method returns.
    ///
    /// ## Return value
    ///
    /// Returns an [`AppendReciept`] containing the segment name and the
    /// **high-water mark offset** — the byte position immediately after the
    /// last written byte. The client stores this value and uses it as the
    /// `offset` parameter on its next streaming read.
    async fn append(
        &mut self,
        user_id: i64,
        payload: Bytes,
    ) -> Result<AppendReciept> {
        let active = match self.active_users.entry(user_id) {
            Entry::Occupied(segment) => segment.into_mut(),
            Entry::Vacant(entry) => {
                let segment =
                    Segment::load_or_create(&self.vault_path, user_id).await?;
                entry.insert(segment)
            }
        };

        if active.current_size >= 5_000_000 {
            let new_segment = active.rotate(&self.vault_path, user_id).await?;
            *active = new_segment;
        }

        active.file.write_all(&payload).await?;

        active.current_size += u64::try_from(payload.len()).unwrap_or(0);

        let offset = active.current_size;

        Ok(AppendReciept {
            file_name: active.file_name.clone(),
            offset,
        })
    }

    /// Flushes all open segment files to disk and notifies the caller.
    ///
    /// Calls `sync_all()` on every [`Segment`] in `active_users` to guarantee
    /// that the OS page cache is written through to permanent storage before the
    /// process exits. Errors from individual `sync_all` calls are silently
    /// discarded to ensure all handles are attempted even if one fails.
    ///
    /// Sends `()` on `reply` after all flushes are complete.
    async fn shutdown(mut self, reply: Sender<()>) {
        for segment in self.active_users.values_mut() {
            let _ = segment.file.sync_all().await;
        }

        let _ = reply.send(());
    }
}

/// Parses the numeric segment ID from a filename of the form `delta_<digits>`.
///
/// Returns `None` if the prefix is missing or the suffix cannot be parsed as
/// a `u32`. Used internally to compare segment IDs during prune validation.
fn parse_segment_id(file_name: &str) -> Option<u32> {
    file_name.strip_prefix("delta_")?.parse().ok()
}
