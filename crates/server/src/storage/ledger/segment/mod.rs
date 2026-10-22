pub mod header;
pub mod recovery;

use std::path::{Path, PathBuf};
use tokio::{
    fs::{self, File, OpenOptions},
    io::AsyncWriteExt,
};

use crate::storage::ledger::{
    AppendReceipt,
    Error::{self, CorruptedSegment},
    Result,
};

pub use header::Header;
pub use recovery::{ACTIVE_FILE_NAME, discover_start_seq, recover_active_seq};

pub const ROTATION_SIZE_THRESHOLD: u64 = 2_000_000; // 2 MB

/// Active append-only writer handle for a specific user and namespace.
#[derive(Debug)]
pub struct ActiveSegment {
    pub file: File,
    pub dir: PathBuf,
    pub current_size: u64,
    pub first_seq: u64,
    pub next_seq: u64,
}

impl ActiveSegment {
    /// Opens the active segment for a namespace directory, or creates a fresh one.
    pub async fn load_or_create(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).await.map_err(Error::IoError)?;

        let current_path = dir.join(ACTIVE_FILE_NAME);

        let mut file = match OpenOptions::new()
            .append(true)
            .read(true)
            .open(&current_path)
            .await
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let start_seq = discover_start_seq(dir).await?;
                return Self::create_fresh(dir, start_seq).await;
            }
            Err(e) => return Err(Error::IoError(e)),
        };

        let metadata = file.metadata().await.map_err(Error::IoError)?;
        let file_len = metadata.len();
        if file_len < Header::SIZE_U64 {
            return Err(CorruptedSegment(
                "Segment file smaller than 500-byte header",
            ));
        }

        let header = Header::read_from(&mut file).await?;
        let next_seq =
            recover_active_seq(&mut file, file_len, header.first_seq).await?;

        Ok(Self {
            file,
            dir: dir.to_path_buf(),
            current_size: file_len,
            first_seq: header.first_seq,
            next_seq,
        })
    }

    /// Atomically creates a fresh `current.log` initialized with a 500-byte header.
    pub async fn create_fresh(dir: &Path, first_seq: u64) -> Result<Self> {
        let current_path = dir.join(ACTIVE_FILE_NAME);

        // Remove ghost file if previously left behind (ignore NotFound)
        let _ = fs::remove_file(&current_path).await;

        let mut file = OpenOptions::new()
            .create_new(true)
            .append(true)
            .read(true)
            .open(&current_path)
            .await
            .map_err(Error::IoError)?;

        let header = Header::new(first_seq);
        header.write_to(&mut file).await?;
        file.sync_data().await.map_err(Error::IoError)?;

        Ok(Self {
            file,
            dir: dir.to_path_buf(),
            current_size: Header::SIZE_U64,
            first_seq,
            next_seq: first_seq,
        })
    }

    /// Appends a raw CRDT payload to the segment using Big-Endian wire framing.
    pub async fn append(&mut self, payload: &[u8]) -> Result<AppendReceipt> {
        let len = u32::try_from(payload.len())?;
        let seq = self.next_seq;

        // Wire frame: [len: 4B BE][seq: 8B BE][payload]
        let mut frame = Vec::with_capacity(4 + 8 + payload.len());
        frame.extend_from_slice(&len.to_be_bytes());
        frame.extend_from_slice(&seq.to_be_bytes());
        frame.extend_from_slice(payload);

        self.file.write_all(&frame).await.map_err(Error::IoError)?;
        self.current_size += u64::try_from(frame.len())?;
        self.next_seq += 1;

        Ok(AppendReceipt {
            sequence_id: seq,
            file_name: ACTIVE_FILE_NAME.to_string(),
            offset: self.current_size,
        })
    }

    #[must_use]
    pub const fn should_rotate(&self) -> bool {
        self.current_size >= ROTATION_SIZE_THRESHOLD
    }

    /// Rotates the segment in place: syncs old file, renames it to `{first_seq:016x}.log`,
    /// and opens a fresh `current.log` starting at `self.next_seq`.
    pub async fn rotate(&mut self) -> Result<()> {
        self.file.sync_all().await.map_err(Error::IoError)?;

        let sealed_name = format!("{:016x}.log", self.first_seq);
        let sealed_path = self.dir.join(&sealed_name);
        let current_path = self.dir.join(ACTIVE_FILE_NAME);

        fs::rename(&current_path, sealed_path)
            .await
            .map_err(Error::IoError)?;

        let fresh = Self::create_fresh(&self.dir, self.next_seq).await?;
        *self = fresh;

        Ok(())
    }

    pub async fn sync(&self) -> Result<()> {
        self.file.sync_all().await.map_err(Error::IoError)
    }
}
