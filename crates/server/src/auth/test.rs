#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_wraps)]

use super::*;
use anyhow::Context;
use kosh_core::config::Config;
use std::{os::unix::fs::PermissionsExt, path::PathBuf};
use tmpdir::TmpDir;

fn dummy_config(vault_path: PathBuf) -> Config {
    Config {
        vault_path,
        port: 6969,
        host: "127.0.0.1".to_string(),
        enable_tls: false,
        socket_path: "/tmp/kosh.sock".to_string(),
    }
}

#[test]
fn token_len_constant_correctness() -> anyhow::Result<()> {
    // Test: verify TOKEN_LEN constant matches wire specification breakdown
    assert_eq!(TOKEN_LEN, 118);
    // 36 (UUID) + 1 (.) + 16 (hex timestamp) + 1 (.) + 64 (hex MAC)
    assert_eq!(TOKEN_LEN, 36 + 1 + 16 + 1 + 64);
    Ok(())
}

#[test]
fn secret_new_and_key_accessors() -> anyhow::Result<()> {
    let raw = [42u8; 32];
    let secret = Secret::new(raw);
    // Test: verify secret key accessor methods return original raw bytes
    assert_eq!(secret.key(), raw);
    assert_eq!(secret.as_bytes(), &raw[..]);
    Ok(())
}

#[test]
fn secret_random_entropy() -> anyhow::Result<()> {
    let s1 = Secret::random();
    let s2 = Secret::random();
    // Test: verify random secrets are non-zero and distinct across invocations
    assert_ne!(s1.key(), [0u8; 32]);
    assert_ne!(s1.key(), s2.key());
    Ok(())
}

#[tokio::test]
async fn secret_load_or_create_fresh_and_permissions() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-auth")
        .await
        .context("failed to create temporary directory")?;
    let config = dummy_config(temp_dir.to_path_buf());

    let secret = Secret::load_or_create(&config)
        .await
        .context("load_or_create fresh")?;

    let secret_file = config.secret_path();
    // Test: verify secret file is created on disk
    assert!(secret_file.exists());

    let metadata = std::fs::metadata(&secret_file).context("file metadata")?;
    let permissions = metadata.permissions();
    let mode = permissions.mode() & 0o777;
    // Test: verify secret file is restricted to owner-only read/write (0o600)
    assert_eq!(
        mode, 0o600,
        "Secret file must have strict 0o600 permissions"
    );

    let bytes = std::fs::read(&secret_file).context("read secret file")?;
    // Test: verify secret file length and content match created secret
    assert_eq!(bytes.len(), 32);
    assert_eq!(bytes.as_slice(), secret.as_bytes());

    Ok(())
}

#[tokio::test]
async fn secret_load_or_create_persists() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-auth")
        .await
        .context("failed to create temporary directory")?;
    let config = dummy_config(temp_dir.to_path_buf());

    let s1 = Secret::load_or_create(&config)
        .await
        .context("initial load_or_create")?;
    let s2 = Secret::load_or_create(&config)
        .await
        .context("subsequent load_or_create")?;

    // Test: verify secret key persists and remains identical across loads
    assert_eq!(s1.key(), s2.key());

    Ok(())
}

#[tokio::test]
async fn secret_corrupted_file_rejected() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-auth")
        .await
        .context("failed to create temporary directory")?;
    let config = dummy_config(temp_dir.to_path_buf());
    let secret_file = config.secret_path();

    // Write incomplete secret (< 32 bytes)
    tokio::fs::write(&secret_file, b"too_short")
        .await
        .context("write corrupt")?;

    let result = Secret::load_or_create(&config).await;
    // Test: verify truncated secret file triggers error instead of loading
    assert!(result.is_err());
    assert!(matches!(result, Err(crate::error::boot::Error::Secret(_))));

    Ok(())
}

#[test]
fn secret_debug_redaction() -> anyhow::Result<()> {
    let raw = [7u8; 32];
    let secret = Secret::new(raw);
    let debug_str = format!("{secret:?}");

    // Test: verify secret Debug formatting redacts raw bytes
    assert!(debug_str.contains("* * *"));
    assert!(!debug_str.contains("7, 7, 7"));
    assert!(!debug_str.contains("070707"));

    Ok(())
}
