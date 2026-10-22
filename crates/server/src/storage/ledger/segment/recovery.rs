use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{self, File},
    io::{AsyncReadExt, AsyncSeekExt},
};

use crate::storage::ledger::{Error, Result, segment::header::Header};

pub const ACTIVE_FILE_NAME: &str = "current.log";

/// 12-byte wire frame prefix prepended to every entry: `[len: u32 BE][seq: u64 BE]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryHeader {
    pub payload_len: u32,
    pub sequence_id: u64,
}

impl EntryHeader {
    pub const SIZE: usize = 12;
    pub const SIZE_U64: u64 = 12;

    /// Reads and decodes a 12-byte entry header.
    /// Returns `Ok(None)` on clean EOF or torn header writes at EOF.
    pub async fn read<R: AsyncReadExt + Unpin>(
        reader: &mut R,
    ) -> Result<Option<Self>> {
        let mut buf = [0_u8; Self::SIZE];

        match reader.read_exact(&mut buf).await {
            Ok(_) => {
                let payload_len =
                    u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                let sequence_id = u64::from_be_bytes([
                    buf[4], buf[5], buf[6], buf[7], buf[8], buf[9], buf[10],
                    buf[11],
                ]);

                Ok(Some(Self {
                    payload_len,
                    sequence_id,
                }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(Error::IoError(e)),
        }
    }
}

/// A sealed, read-only historical segment file (`{start_seq:016x}.log`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedSegment {
    pub start_seq: u64,
    pub path: PathBuf,
}

impl SealedSegment {
    /// Parses a filename into a `SealedSegment` if it matches `{hex:016x}.log`.
    #[must_use]
    pub fn parse(path: PathBuf) -> Option<Self> {
        let name = path.file_name()?.to_str()?;
        if name == ACTIVE_FILE_NAME {
            return None;
        }
        let hex = name.strip_suffix(".log")?;
        let start_seq = u64::from_str_radix(hex, 16).ok()?;
        Some(Self { start_seq, path })
    }

    /// Scans a directory and returns all sealed segments sorted by sequence.
    pub async fn list(dir: &Path) -> Result<Vec<Self>> {
        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(e) => return Err(Error::IoError(e)),
        };

        let mut segments = Vec::new();
        while let Some(entry) =
            entries.next_entry().await.map_err(Error::IoError)?
        {
            if let Some(sealed) = Self::parse(entry.path()) {
                segments.push(sealed);
            }
        }
        segments.sort_unstable_by_key(|s| s.start_seq);
        Ok(segments)
    }

    /// Returns the sealed segment with the highest starting sequence in `dir`.
    pub async fn highest(dir: &Path) -> Result<Option<Self>> {
        let segments = Self::list(dir).await?;
        Ok(segments.into_iter().last())
    }
}

/// Sequentially scans records in an active segment file.
///
/// Validates framing, recovers `next_seq`, and truncates trailing torn writes.
pub async fn recover_active_seq(
    file: &mut File,
    file_len: u64,
    first_seq: u64,
) -> Result<u64> {
    file.seek(SeekFrom::Start(Header::SIZE_U64))
        .await
        .map_err(Error::IoError)?;

    let mut last_seq: Option<u64> = None;
    let mut valid_offset = Header::SIZE_U64;

    while let Some(header) = EntryHeader::read(file).await? {
        let frame_total_len =
            EntryHeader::SIZE_U64 + u64::from(header.payload_len);

        // Torn write guard: payload truncated mid-write during crash
        if valid_offset + frame_total_len > file_len {
            break;
        }

        // Seek past payload
        file.seek(SeekFrom::Current(i64::from(header.payload_len)))
            .await
            .map_err(Error::IoError)?;

        valid_offset += frame_total_len;
        last_seq = Some(header.sequence_id);
    }

    // Torn write self-healing: truncate any trailing partial bytes
    if file_len > valid_offset {
        file.set_len(valid_offset).await.map_err(Error::IoError)?;
        file.sync_all().await.map_err(Error::IoError)?;
    }

    // Position write cursor at clean EOF for subsequent appends
    file.seek(SeekFrom::Start(valid_offset))
        .await
        .map_err(Error::IoError)?;

    Ok(last_seq.map_or(first_seq, |s| s + 1))
}

/// Discovers the next starting sequence when `current.log` does not exist.
pub async fn discover_start_seq(dir: &Path) -> Result<u64> {
    let Some(highest) = SealedSegment::highest(dir).await? else {
        return Ok(1); // Brand-new ledger partition starts at sequence 1
    };

    let mut file = File::open(&highest.path).await.map_err(Error::IoError)?;
    let file_len = file.metadata().await.map_err(Error::IoError)?.len();
    let header = Header::read_from(&mut file).await?;

    recover_active_seq(&mut file, file_len, header.first_seq).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sealed_seq() {
        assert_eq!(
            SealedSegment::parse(PathBuf::from("0000000000000000.log")),
            Some(SealedSegment {
                start_seq: 0,
                path: PathBuf::from("0000000000000000.log")
            })
        );
        assert_eq!(
            SealedSegment::parse(PathBuf::from("00000000000003e8.log")),
            Some(SealedSegment {
                start_seq: 1000,
                path: PathBuf::from("00000000000003e8.log")
            })
        );
        assert_eq!(SealedSegment::parse(PathBuf::from("current.log")), None);
        assert_eq!(SealedSegment::parse(PathBuf::from("random.txt")), None);
    }
}
