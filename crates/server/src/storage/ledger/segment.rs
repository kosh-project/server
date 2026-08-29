use std::{
    fs::metadata,
    io::{self, SeekFrom},
    path::Path,
};

use crate::storage::ledger::{Error, Result};

use tokio::{
    fs::{self, File, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
};

#[derive(Debug)]
pub struct Segment {
    pub file: File,
    pub file_name: String,
    pub current_size: u64,
}

impl Segment {
    pub async fn load_or_create(
        vault_path: &Path,
        user_id: i64,
    ) -> Result<Self> {
        let dir = vault_path.join("ledgers").join(user_id.to_string());

        fs::create_dir_all(&dir).await?;

        let current_path = dir.join("CURRENT");

        if let Ok(current_data) = fs::read_to_string(&current_path).await
            && let Ok(segment) = Self::load(&dir, current_data).await
        {
            return Ok(segment);
        }

        Self::create(dir, current_path).await
    }

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

    async fn create<P>(dir: P, current: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let file_name = "delta_0000001";
        let path = dir.as_ref().join(file_name);

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(&path)
            .await?;

        // Append Header (i'll decide what exactly)
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

    pub async fn rotate<P>(&self, vault_path: P, user_id: i64) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let dir = vault_path.as_ref().join("ledger").join(user_id.to_string());

        let id = self.file_name.trim_start_matches("delta_");
        let current_id: u32 = id.parse()?;

        let next_id = current_id + 1;

        let file_name = format!("delta_{:07}", next_id);
        let path = dir.join(&file_name);

        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
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

async fn overwrite_header(file: &mut File, prev_id: u32) -> Result<()> {
    let mut header = [0u8; 500];
    header[0..4].copy_from_slice(b"KOSH");
    header[4..6].copy_from_slice(&1u16.to_le_bytes());
    header[6..10].copy_from_slice(&prev_id.to_le_bytes());

    file.write_all(&header).await?;
    file.sync_data().await?;
    Ok(())
}

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

    // In future, maybe we'll confirm if previous delta_{} exists

    file.seek(SeekFrom::End(0)).await?;

    Ok(())
}
