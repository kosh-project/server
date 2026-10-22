use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::storage::ledger::{Error::CorruptedSegment, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub version: u16,
    pub first_seq: u64,
}

impl Header {
    pub const MAGIC: &'static [u8; 4] = b"KOSH";
    pub const CURRENT_VERSION: u16 = 1;
    pub const SIZE: usize = 500;
    pub const SIZE_U64: u64 = 500;
}

impl Header {
    #[must_use]
    pub const fn new(first_seq: u64) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            first_seq,
        }
    }

    #[must_use]
    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(Self::MAGIC);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6..14].copy_from_slice(&self.first_seq.to_le_bytes());
        buf
    }

    pub fn decode(buf: &[u8; Self::SIZE]) -> Result<Self> {
        if &buf[0..4] != Self::MAGIC {
            return Err(CorruptedSegment("Invalid KOSH signature"));
        }

        let version = u16::from_le_bytes([buf[4], buf[5]]);
        if version != Self::CURRENT_VERSION {
            return Err(CorruptedSegment("Unsupported segment header version"));
        }

        let first_seq = u64::from_le_bytes([
            buf[6], buf[7], buf[8], buf[9], buf[10], buf[11], buf[12], buf[13],
        ]);
        Ok(Self { version, first_seq })
    }

    pub async fn read_from<R: AsyncReadExt + Unpin>(
        reader: &mut R,
    ) -> Result<Self> {
        let mut buf = [0u8; Self::SIZE];
        reader.read_exact(&mut buf).await?;
        Self::decode(&buf)
    }

    pub async fn write_to<W: AsyncWriteExt + Unpin>(
        &self,
        writer: &mut W,
    ) -> Result<()> {
        let buf = self.encode();
        writer.write_all(&buf).await?;
        writer.flush().await?;
        Ok(())
    }
}
