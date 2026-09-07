use std::any;

use anyhow::Result;
use bytes::Bytes;
use tmpdir::TmpDir;
use tokio::fs;
use tokio::io::AsyncReadExt;

use crate::storage::ledger::segment::Segment;
use crate::storage::ledger::{Error, Handle, handle};

type UserId = i64;

async fn set_env() -> Result<(TmpDir, UserId)> {
    let tmp = TmpDir::new("ledger_tests").await?;

    Ok((tmp, 99))
}

#[tokio::test]
async fn create_and_load_persistance() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;

    let vault_path = dir.to_path_buf();

    let segment = Segment::load_or_create(&vault_path, user_id).await?;

    assert_eq!(segment.file_name, "delta_0000001");
    assert_eq!(segment.current_size, 500);

    let current_path = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join("CURRENT");
    let current_delta = fs::read_to_string(current_path).await?;
    assert_eq!(current_delta, "delta_0000001");

    let loaded = Segment::load_or_create(&vault_path, user_id).await?;
    assert_eq!(loaded.file_name, "delta_0000001");
    assert_eq!(loaded.current_size, 500);

    Ok(())
}

#[tokio::test]
async fn ghost_file_self_handle() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;

    let vault_path = dir.to_path_buf();

    let ledger_dir = vault_path.join("ledgers").join(user_id.to_string());
    fs::create_dir_all(&ledger_dir).await?;

    let delta_path = ledger_dir.join("delta_0000001");
    fs::write(&delta_path, b"GARBAGE DATA PROBABLY FROM PREVIOUS CRASH")
        .await?;

    let segment = Segment::load_or_create(&vault_path, user_id).await?;
    assert_eq!(segment.file_name, "delta_0000001");
    assert_eq!(segment.current_size, 500);

    Ok(())
}

#[tokio::test]
async fn rotation_physics() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let mut segment = Segment::load_or_create(&vault_path, user_id).await?;

    let new_segment = segment.rotate(&vault_path, user_id).await?;

    assert_eq!(new_segment.file_name, "delta_0000002");
    assert_eq!(new_segment.current_size, 500);

    let current_path = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join("CURRENT");
    let current_content = fs::read_to_string(current_path).await?;

    assert_eq!(current_content, "delta_0000002");

    Ok(())
}

use crate::storage::ledger::Error::CorruptedSegment;

#[tokio::test]
async fn verify_corrupted_head() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    Segment::load_or_create(&vault_path, user_id).await?;

    let path = vault_path
        .join("ledgers")
        .join(user_id.to_string())
        .join("delta_0000001");
    let file = fs::OpenOptions::new().write(true).open(&path).await?;
    file.set_len(100).await?;
    drop(file);

    let result = Segment::load_or_create(&vault_path, user_id).await;

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

    let receipt = handle.append(user_id, payload.clone()).await?;

    assert_eq!(receipt.file_name, "delta_0000001");
    assert_eq!(receipt.offset, 521);

    let receipt = handle.append(user_id, payload.clone()).await?;
    assert_eq!(receipt.file_name, "delta_0000001");

    assert_eq!(receipt.offset, 542);

    Ok(())
}

#[tokio::test]
async fn actor_rotation_on_limit() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());

    let big_payload = Bytes::from(vec![0u8; 5_000_000]);

    let reciept_a = handle.append(user_id, big_payload.clone()).await?;

    assert_eq!(reciept_a.file_name, "delta_0000001");

    let payload = Bytes::from("tiny mini payload");
    let reciept_b = handle.append(user_id, payload).await?;

    assert_ne!(reciept_b.file_name, reciept_a.file_name);

    assert_eq!(reciept_b.file_name, "delta_0000002");

    Ok(())
}

#[tokio::test]
async fn actor_prune_logic() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());

    let big_payload = Bytes::from(vec![0u8; 5_000_000]);

    handle.append(user_id, big_payload.clone()).await?;
    handle.append(user_id, big_payload.clone()).await?;
    handle
        .append(user_id, Bytes::from("tiny mini payload"))
        .await?;

    handle.prune(user_id, 2).await?;

    let delta_dir = vault_path.join("ledgers").join(user_id.to_string());

    assert!(!delta_dir.join("delta_0000001").exists());
    assert!(!delta_dir.join("delta_0000002").exists());
    assert!(delta_dir.join("delta_0000003").exists());

    let result = handle.prune(user_id, 99).await;

    assert!(
        matches!(result, Err(Error::InvalidPrune)),
        "Expected Error::InvalidPrune found {result:?}"
    );

    assert!(
        delta_dir.join("delta_0000003").exists(),
        "Active file must not be pruned!"
    );

    Ok(())
}

#[tokio::test]
async fn handle_read_segment_success() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());
    handle
        .append(user_id, Bytes::from("PAYLOAD PAYLOAD"))
        .await?;

    let mut file = handle
        .read_segment(&vault_path, user_id, "delta_0000001", 0)
        .await?;

    let mut buf = String::new();
    file.read_to_string(&mut buf).await?;

    assert_eq!(buf, "PAYLOAD PAYLOAD");

    Ok(())
}

#[tokio::test]
async fn handle_read_out_of_bounds() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();

    let handle = Handle::spawn(vault_path.clone());

    handle.append(user_id, Bytes::from("tiny")).await?;

    let res = handle
        .read_segment(&vault_path, user_id, "delta_0000001", 9999)
        .await;

    assert!(matches!(res, Err(Error::InvalidOffset)));
    Ok(())
}

#[tokio::test]
async fn read_path_traversal_blocked() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let handle = Handle::spawn(vault_path.clone());

    let res_a = handle
        .read_segment(&vault_path, user_id, "../delta_0000001", 500)
        .await;
    assert!(matches!(res_a, Err(Error::InvalidFileName)));

    let res_b = handle
        .read_segment(
            &vault_path,
            user_id,
            "delta_0000001/../../../etc/passwd",
            500,
        )
        .await;
    assert!(matches!(res_b, Err(Error::InvalidFileName)));

    Ok(())
}

#[tokio::test]
async fn handle_read_enforces_500_byte_floor() -> anyhow::Result<()> {
    let (dir, user_id) = set_env().await?;
    let vault_path = dir.to_path_buf();
    let handle = Handle::spawn(vault_path.clone());

    handle.append(user_id, Bytes::from("PAYLOAD SAUCE")).await?;

    let mut file = handle
        .read_segment(vault_path, user_id, "delta_0000001", 0)
        .await?;

    let mut buf = String::new();
    file.read_to_string(&mut buf).await?;
    assert_eq!(buf, "PAYLOAD SAUCE");

    Ok(())
}
