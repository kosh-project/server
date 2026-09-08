use std::{
    io::{ErrorKind, SeekFrom},
    path::Path,
};

use crate::storage::ledger::{Error, Result};

use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};

/// An open handle to a single delta segment file.
///
/// A segment is the unit of rotation in the ledger. When the active segment
/// grows beyond 5 MB, [`Committer`] calls [`Segment::rotate`] to atomically
/// seal the current file and open a new one. Each segment is identified by a
/// zero-padded decimal suffix, e.g. `delta_0000001`, `delta_0000002`.
///
/// `Segment` owns the open [`File`] handle and tracks the current file size
/// so the [`Committer`] can decide when to rotate without an extra `stat(2)`
/// call on every write.
///
/// [`Committer`]: crate::storage::ledger::committer::Committer
#[derive(Debug)]
pub struct Segment {
    /// The open file handle used for all append and read operations.
    ///
    /// Opened with both `append` and `read` permissions so that the
    /// [`Handle::read_segment`] path can seek and read without requiring a
    /// separate file descriptor.
    ///
    /// [`Handle::read_segment`]: crate::storage::ledger::handle::Handle::read_segment
    pub file: File,
    /// The bare filename of this segment, e.g. `"delta_0000003"`.
    ///
    /// This string is also written into the `CURRENT` pointer file to allow
    /// the server to resume from the correct segment after a restart.
    pub file_name: String,
    /// The current total size of the segment file in bytes, including the
    /// 500-byte header.
    ///
    /// Maintained in memory to avoid a `stat(2)` call on every append. The
    /// [`Committer`] uses this field to decide whether a rotation is needed.
    ///
    /// [`Committer`]: crate::storage::ledger::committer::Committer
    pub current_size: u64,
}

impl Segment {
    /// Opens the active segment for a user's ledger, or creates a brand-new one.
    ///
    /// This is the entry point called by the [`Committer`] when it receives the
    /// first [`Action::Append`] for a user that is not yet in its in-memory map.
    ///
    /// ## Decision logic
    ///
    /// 1. The user's ledger directory (`vault/ledgers/<user_id>/`) is created
    ///    if it does not exist.
    /// 2. The `CURRENT` pointer file is read.
    ///    - If found, the named segment is opened via [`load`].
    ///    - If `NotFound`, this is a brand-new ledger. [`create`] is called.
    ///    - Any other IO error (e.g. `PermissionDenied`) is returned immediately
    ///      as [`Error::IoError`] rather than silently treated as a missing file.
    ///
    /// ## Errors
    ///
    /// Returns `Err` if:
    /// - The directory cannot be created (`PermissionDenied` or similar).
    /// - The `CURRENT` file exists but cannot be read.
    /// - The segment file referenced by `CURRENT` is corrupt or too small
    ///   to contain a valid header ([`Error::CorruptedSegment`]).
    ///
    /// [`Committer`]: crate::storage::ledger::committer::Committer
    /// [`Action::Append`]: crate::storage::ledger::action::Action::Append
    /// [`load`]: Self::load
    /// [`create`]: Self::create
    pub async fn load_or_create(
        vault_path: &Path,
        user_id: i64,
    ) -> Result<Self> {
        let dir = vault_path.join("ledgers").join(user_id.to_string());

        fs::create_dir_all(&dir).await?;

        let current_path = dir.join("CURRENT");

        match fs::read_to_string(&current_path).await {
            Ok(current_data) => Self::load(dir, current_data).await,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                Self::create(dir, current_path).await
            }
            Err(e) => Err(Error::IoError(e)),
        }
    }

    /// Opens an existing segment file by name and validates its header.
    ///
    /// The file is opened in append-and-read mode so that subsequent writes
    /// are always appended and seeks for read operations work correctly.
    ///
    /// [`verify_header`] is called before returning. If the file is smaller
    /// than 500 bytes or does not start with the `KOSH` magic bytes, the
    /// segment is considered corrupt and [`Error::CorruptedSegment`] is returned.
    /// The caller is responsible for deciding whether to abort or recover.
    ///
    /// [`verify_header`]: verify_header
    async fn load<S, P>(dir: P, name: S) -> Result<Self>
    where
        S: AsRef<str>,
        P: AsRef<Path>,
    {
        let file_name = name.as_ref().trim();
        let path = dir.as_ref().join(file_name);

        let mut file = OpenOptions::new()
            .append(true)
            .read(true)
            .open(&path)
            .await?;

        let metadata = file.metadata().await?;

        verify_header(&mut file).await?;
        Ok(Self {
            file,
            file_name: file_name.to_owned(),
            current_size: metadata.len(),
        })
    }

    /// Creates the first segment (`delta_0000001`) for a brand-new ledger.
    ///
    /// ## Ghost file self-healing
    ///
    /// Before creating the file, `fs::remove_file` is called on the target
    /// path and the error is silently discarded. This handles the case where
    /// the server crashed after writing the file but before updating `CURRENT`,
    /// leaving behind a ghost file with an unknown or corrupt state. The ghost
    /// is wiped and a pristine segment is written in its place.
    ///
    /// `OpenOptions::create_new(true)` is used after the explicit deletion to
    /// prevent any race condition from silently overwriting data.
    ///
    /// ## CURRENT pointer
    ///
    /// The `CURRENT` file is updated atomically by writing to `CURRENT.tmp`
    /// and then renaming it. This ensures the pointer is never in a partially
    /// written state if the process is killed mid-write.
    async fn create<P>(dir: P, current: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let file_name = "delta_0000001";
        let path = dir.as_ref().join(file_name);

        // Silently remove any ghost file left behind by a previous crash.
        let _ = fs::remove_file(&path).await;

        let mut file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .read(true)
            .open(&path)
            .await?;

        overwrite_header(&mut file, 0).await?;

        let current_tmp = dir.as_ref().join("CURRENT.tmp");
        fs::write(&current_tmp, file_name).await?;
        fs::rename(&current_tmp, current.as_ref()).await?;

        Ok(Self {
            file,
            file_name: file_name.to_owned(),
            current_size: 500,
        })
    }

    /// Seals the current segment and opens the next one.
    ///
    /// Called by the [`Committer`] when `current_size` reaches the 5 MB
    /// rotation threshold. The new segment's ID is `current_id + 1`, formatted
    /// as a zero-padded 7-digit decimal, e.g. `delta_0000004`.
    ///
    /// The new segment's header records `current_id` in the `prev_id` field so
    /// that a future integrity checker could walk the chain backwards and detect
    /// gaps caused by accidental deletion.
    ///
    /// Like [`create`], any ghost file at the target path is removed first, and
    /// the `CURRENT` pointer is updated atomically via a `.tmp` rename.
    ///
    /// ## Errors
    ///
    /// Returns `Err` if:
    /// - The current segment's filename cannot be parsed as `delta_<u32>`.
    /// - The new file cannot be created (disk full, permissions, etc.).
    /// - The `CURRENT` pointer cannot be written or renamed.
    ///
    /// [`Committer`]: crate::storage::ledger::committer::Committer
    /// [`create`]: Self::create
    pub async fn rotate<P>(&self, vault_path: P, user_id: i64) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let dir = vault_path
            .as_ref()
            .join("ledgers")
            .join(user_id.to_string());

        let id = self.file_name.trim_start_matches("delta_");
        let current_id: u32 = id.parse()?;

        let next_id = current_id + 1;

        let file_name = format!("delta_{next_id:07}");
        let path = dir.join(&file_name);

        // Remove any ghost file left behind by a previous crash before this slot.
        let _ = fs::remove_file(&path).await;

        let mut file = OpenOptions::new()
            .append(true)
            .create_new(true)
            .read(true)
            .open(&path)
            .await?;

        overwrite_header(&mut file, current_id).await?;

        let current_tmp = dir.join("CURRENT.tmp");
        fs::write(&current_tmp, &file_name).await?;
        fs::rename(&current_tmp, dir.join("CURRENT")).await?;

        Ok(Self {
            file,
            file_name,
            current_size: 500,
        })
    }
}

/// Writes the 500-byte KOSH header to the beginning of a newly created segment file.
///
/// ## Header layout
///
/// | Offset | Size | Content |
/// |--------|------|---------|
/// | 0      | 4    | `b"KOSH"` magic bytes |
/// | 4      | 2    | Format version `1u16`, little-endian |
/// | 6      | 4    | `prev_id` — the ID of the previous segment, `0` for the first |
/// | 10     | 490  | Reserved, zeroed |
///
/// `sync_data()` is called after writing to ensure the header is durable on
/// disk before the `CURRENT` pointer is updated.
async fn overwrite_header(file: &mut File, prev_id: u32) -> Result<()> {
    let mut header = [0u8; 500];
    header[0..4].copy_from_slice(b"KOSH");
    header[4..6].copy_from_slice(&1u16.to_le_bytes());
    header[6..10].copy_from_slice(&prev_id.to_le_bytes());

    file.write_all(&header).await?;
    file.sync_data().await?;
    Ok(())
}

/// Validates that an existing segment file starts with a well-formed KOSH header.
///
/// This is called by [`load`] before returning a segment to the [`Committer`].
/// The file cursor is seeked back to the end after validation so that subsequent
/// append writes land at the correct position.
///
/// ## Validation steps
///
/// 1. The file must be at least 500 bytes long. A shorter file indicates a
///    truncated write from a previous crash.
/// 2. The first 4 bytes must be the ASCII literal `KOSH`. Any other value means
///    the file is not a valid ledger segment (possibly a leftover from another tool).
///
/// ## Errors
///
/// Returns [`Error::CorruptedSegment`] with a human-readable reason string if
/// either check fails.
///
/// [`load`]: Segment::load
/// [`Committer`]: crate::storage::ledger::committer::Committer
async fn verify_header(file: &mut File) -> Result<()> {
    let metadata = file.metadata().await?;

    if metadata.len() < 500 {
        return Err(Error::CorruptedSegment(
            "Smaller than 500-bytes header".into(),
        ));
    }

    let mut header = [0u8; 10];
    file.seek(SeekFrom::Start(0)).await?;
    file.read_exact(&mut header).await?;

    if header[0..4] != *b"KOSH" {
        return Err(Error::CorruptedSegment("Invalid KOSH Signature".into()));
    }

    // In future, the prev_id field (bytes 6..10) could be used to confirm
    // that the preceding segment file still exists, catching accidental deletions.

    file.seek(SeekFrom::End(0)).await?;

    Ok(())
}
