#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::as_conversions)]

use anyhow::{Context, Result};
use bytes::Bytes;
use tmpdir::TmpDir;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::storage::ledger::Error::{self, CorruptedSegment};
use crate::storage::ledger::segment::{ACTIVE_FILE_NAME, ActiveSegment};
use crate::storage::ledger::{Handle, Namespace};

type UserId = i64;

async fn set_env() -> Result<(TmpDir, UserId)> {
    let tmp = TmpDir::new("ledger_tests")
        .await
        .context("failed to create temporary directory")?;

    Ok((tmp, 99))
}

#[tokio::test]
async fn create_and_load_persistance() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let ledger_dir = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join(Namespace::Files.as_str());

    let segment = ActiveSegment::load_or_create(&ledger_dir)
        .await
        .context("failed to create segment")?;

    // Test: verify new segment initializes with header size 500 and sequence 1
    assert_eq!(segment.current_size, 500);
    assert_eq!(segment.first_seq, 1);
    assert_eq!(segment.next_seq, 1);

    let current_path = ledger_dir.join(ACTIVE_FILE_NAME);
    // Test: verify current.log exists on disk
    assert!(current_path.exists());

    let loaded = ActiveSegment::load_or_create(&ledger_dir)
        .await
        .context("failed to load existing segment")?;
    // Test: verify reloading existing segment preserves sequence and size
    assert_eq!(loaded.current_size, 500);
    assert_eq!(loaded.first_seq, 1);
    assert_eq!(loaded.next_seq, 1);

    Ok(())
}

#[tokio::test]
async fn ghost_file_self_handle() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let ledger_dir = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join(Namespace::Files.as_str());

    fs::create_dir_all(&ledger_dir)
        .await
        .context("failed to create ledger directory")?;

    let ghost_path = ledger_dir.join(ACTIVE_FILE_NAME);
    fs::write(&ghost_path, b"GARBAGE DATA PROBABLY FROM PREVIOUS CRASH")
        .await
        .context("failed to write ghost delta file")?;

    let segment = ActiveSegment::create_fresh(&ledger_dir, 1)
        .await
        .context("failed to recover ghost file segment")?;
    // Test: verify ghost file is replaced with a valid 500-byte segment
    assert_eq!(segment.current_size, 500);
    assert_eq!(segment.first_seq, 1);
    assert_eq!(segment.next_seq, 1);

    Ok(())
}

#[tokio::test]
async fn rotation_physics() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let ledger_dir = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join(Namespace::Files.as_str());

    let mut segment = ActiveSegment::load_or_create(&ledger_dir)
        .await
        .context("failed to initialize segment")?;

    segment
        .append(b"payload 1")
        .await
        .context("failed to append to segment")?;

    segment.rotate().await.context("failed to rotate segment")?;

    // Test: verify sealed segment file exists with hex sequence name 0000000000000001.log
    let sealed_name = format!("{:016x}.log", 1);
    assert!(ledger_dir.join(&sealed_name).exists());

    // Test: verify rotated segment starts fresh with next sequence
    assert_eq!(segment.first_seq, 2);
    assert_eq!(segment.next_seq, 2);
    assert_eq!(segment.current_size, 500);

    let receipt = segment
        .append(b"payload 2")
        .await
        .context("failed to append after rotation")?;
    assert_eq!(receipt.sequence_id, 2);
    assert_eq!(receipt.file_name, ACTIVE_FILE_NAME);

    Ok(())
}

#[tokio::test]
async fn verify_corrupted_head() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let ledger_dir = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join(Namespace::Files.as_str());

    ActiveSegment::load_or_create(&ledger_dir)
        .await
        .context("failed to initialize segment")?;

    let path = ledger_dir.join(ACTIVE_FILE_NAME);
    let file = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .await
        .context("failed to open segment for tampering")?;
    file.set_len(100)
        .await
        .context("failed to truncate segment file")?;
    drop(file);

    let result = ActiveSegment::load_or_create(&ledger_dir).await;

    // Test: verify segment with invalid length below 500 bytes returns CorruptedSegment error
    assert!(
        matches!(result, Err(CorruptedSegment(_))),
        "Expected Error::CorruptedSegment(_) found {result:?}"
    );

    Ok(())
}

#[tokio::test]
async fn test_actor_append() -> anyhow::Result<()> {
    let (tmp_dir, user_id) = set_env().await?;
    let vault_path = tmp_dir.to_path_buf();

    let handle = Handle::spawn(vault_path);

    let payload = Bytes::from("ENCRYPTED ACTION LOGS");

    let receipt = handle
        .append(user_id, Namespace::Files, payload.clone())
        .await
        .context("failed to append first payload")?;

    // Test: verify initial append starts at header offset + frame header (500 + 12 + payload len)
    assert_eq!(receipt.file_name, "current.log");
    assert_eq!(receipt.sequence_id, 1);
    assert_eq!(receipt.offset, 500 + 12 + payload.len() as u64);

    let receipt = handle
        .append(user_id, Namespace::Files, payload.clone())
        .await
        .context("failed to append second payload")?;
    // Test: verify subsequent append increments sequence and byte offset
    assert_eq!(receipt.file_name, "current.log");
    assert_eq!(receipt.sequence_id, 2);
    assert_eq!(receipt.offset, 500 + (12 + payload.len() as u64) * 2);

    Ok(())
}

#[tokio::test]
async fn actor_rotation_on_limit() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());

    let big_payload = Bytes::from(vec![0u8; 2_000_000]);

    let receipt_a = handle
        .append(user_id, Namespace::Files, big_payload)
        .await
        .context("failed to append oversized payload")?;

    // Test: verify initial large write lands in current.log with sequence 1
    assert_eq!(receipt_a.file_name, "current.log");
    assert_eq!(receipt_a.sequence_id, 1);

    let payload = Bytes::from("tiny mini payload");
    let receipt_b = handle
        .append(user_id, Namespace::Files, payload)
        .await
        .context("failed to append payload after threshold")?;

    // Test: verify exceeding segment size threshold rotated old segment and appended with sequence 2
    assert_eq!(receipt_b.file_name, "current.log");
    assert_eq!(receipt_b.sequence_id, 2);

    let sealed_path = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join(Namespace::Files.as_str())
        .join(format!("{:016x}.log", 1));
    assert!(sealed_path.exists(), "Sealed segment must exist on disk");

    Ok(())
}

#[tokio::test]
async fn actor_prune_logic() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());

    let big_payload = Bytes::from(vec![0u8; 2_000_000]);

    // Segment 1 (seq 1) -> triggers rotate
    handle
        .append(user_id, Namespace::Files, big_payload.clone())
        .await
        .context("failed to append delta 1")?;
    // Segment 2 (seq 2) -> triggers rotate
    handle
        .append(user_id, Namespace::Files, big_payload.clone())
        .await
        .context("failed to append delta 2")?;
    // Segment 3 (seq 3)
    handle
        .append(user_id, Namespace::Files, Bytes::from("tiny mini payload"))
        .await
        .context("failed to append delta 3")?;

    let ledger_dir = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join(Namespace::Files.as_str());

    let sealed_1 = ledger_dir.join(format!("{:016x}.log", 1));
    let sealed_2 = ledger_dir.join(format!("{:016x}.log", 2));
    assert!(sealed_1.exists());
    assert!(sealed_2.exists());

    // Prune up to safe sequence 1
    let pruned = handle
        .prune(user_id, Namespace::Files, 1)
        .await
        .context("failed to prune segments up to sequence 1")?;

    // Test: verify 1 segment was pruned
    assert_eq!(pruned, 1);
    assert!(!sealed_1.exists());
    assert!(sealed_2.exists());
    assert!(ledger_dir.join("current.log").exists());

    Ok(())
}

#[tokio::test]
async fn handle_read_segment_success() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());
    let payload = Bytes::from("PAYLOAD PAYLOAD");
    handle
        .append(user_id, Namespace::Files, payload.clone())
        .await
        .context("failed to append test payload")?;

    Handle::shutdown(handle.sender()).await;
    let mut file = handle
        .read_segment(&vault_path, user_id, Namespace::Files, "current.log", 0)
        .await
        .context("failed to read segment")?;

    // Read 4 bytes length + 8 bytes sequence ID + payload
    let mut header_buf = [0u8; 12];
    file.read_exact(&mut header_buf)
        .await
        .context("failed to read entry frame header")?;
    let len = u32::from_be_bytes([
        header_buf[0],
        header_buf[1],
        header_buf[2],
        header_buf[3],
    ]);
    let seq = u64::from_be_bytes([
        header_buf[4],
        header_buf[5],
        header_buf[6],
        header_buf[7],
        header_buf[8],
        header_buf[9],
        header_buf[10],
        header_buf[11],
    ]);
    assert_eq!(len, u32::try_from(payload.len())?);
    assert_eq!(seq, 1);

    let mut data_buf = vec![0u8; len as usize];
    file.read_exact(&mut data_buf)
        .await
        .context("failed to read payload")?;

    // Test: verify segment reader recovers appended payload verbatim
    assert_eq!(data_buf, payload.as_ref());

    Ok(())
}

#[tokio::test]
async fn handle_read_out_of_bounds() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());

    handle
        .append(user_id, Namespace::Files, Bytes::from("tiny"))
        .await
        .context("failed to append payload")?;

    let res = handle
        .read_segment(
            &vault_path,
            user_id,
            Namespace::Files,
            "current.log",
            9999,
        )
        .await;

    // Test: verify reading offset exceeding segment size returns InvalidOffset error
    assert!(matches!(res, Err(Error::InvalidOffset)));
    Ok(())
}

#[tokio::test]
async fn read_path_traversal_blocked() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let handle = Handle::spawn(vault_path.clone());

    let res_a = handle
        .read_segment(
            &vault_path,
            user_id,
            Namespace::Files,
            "../current.log",
            500,
        )
        .await;
    // Test: verify relative parent traversal in segment filename returns InvalidFileName
    assert!(matches!(res_a, Err(Error::InvalidFileName)));

    let res_b = handle
        .read_segment(
            &vault_path,
            user_id,
            Namespace::Files,
            "current.log/../../../etc/passwd",
            500,
        )
        .await;
    // Test: verify deep path traversal in segment filename returns InvalidFileName
    assert!(matches!(res_b, Err(Error::InvalidFileName)));

    Ok(())
}

#[tokio::test]
async fn handle_read_enforces_500_byte_floor() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let handle = Handle::spawn(vault_path.clone());

    let payload = Bytes::from("PAYLOAD SAUCE");
    handle
        .append(user_id, Namespace::Files, payload.clone())
        .await
        .context("failed to append test payload")?;

    Handle::shutdown(handle.sender()).await;

    let mut file = handle
        .read_segment(&vault_path, user_id, Namespace::Files, "current.log", 0)
        .await
        .context("failed to read segment from start")?;

    let mut header_buf = [0u8; 12];
    file.read_exact(&mut header_buf)
        .await
        .context("failed to read frame header past 500 byte floor")?;

    let len = u32::from_be_bytes([
        header_buf[0],
        header_buf[1],
        header_buf[2],
        header_buf[3],
    ]);
    assert_eq!(len, u32::try_from(payload.len())?);

    let mut data_buf = vec![0u8; len as usize];
    file.read_exact(&mut data_buf)
        .await
        .context("failed to read payload")?;

    // Test: verify reading at offset 0 automatically seeks past 500-byte header to payload
    assert_eq!(data_buf, payload.as_ref());

    Ok(())
}
