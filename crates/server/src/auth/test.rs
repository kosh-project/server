#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;
use kosh_core::config::Config;
use std::{
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn unique_temp_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kosh_test_auth_{nonce}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

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
fn token_len_constant_correctness() {
    assert_eq!(TOKEN_LEN, 118);
    // 36 (UUID) + 1 (.) + 16 (hex timestamp) + 1 (.) + 64 (hex MAC)
    assert_eq!(TOKEN_LEN, 36 + 1 + 16 + 1 + 64);
}

#[test]
fn secret_new_and_key_accessors() {
    let raw = [42u8; 32];
    let secret = Secret::new(raw);
    assert_eq!(secret.key(), raw);
    assert_eq!(secret.as_bytes(), &raw[..]);
}

#[test]
fn secret_random_entropy() {
    let s1 = Secret::random();
    let s2 = Secret::random();
    assert_ne!(s1.key(), [0u8; 32]);
    assert_ne!(s1.key(), s2.key());
}

#[tokio::test]
async fn secret_load_or_create_fresh_and_permissions() {
    let temp_dir = unique_temp_dir();
    let config = dummy_config(temp_dir.clone());

    let secret = Secret::load_or_create(&config)
        .await
        .expect("load_or_create fresh");

    let secret_file = config.secret_path();
    assert!(secret_file.exists());

    let metadata = std::fs::metadata(&secret_file).expect("file metadata");
    let permissions = metadata.permissions();
    let mode = permissions.mode() & 0o777;
    assert_eq!(
        mode, 0o600,
        "Secret file must have strict 0o600 permissions"
    );

    let bytes = std::fs::read(&secret_file).expect("read secret file");
    assert_eq!(bytes.len(), 32);
    assert_eq!(bytes.as_slice(), secret.as_bytes());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn secret_load_or_create_persists() {
    let temp_dir = unique_temp_dir();
    let config = dummy_config(temp_dir.clone());

    let s1 = Secret::load_or_create(&config).await.expect("initial");
    let s2 = Secret::load_or_create(&config).await.expect("reload");

    assert_eq!(s1.key(), s2.key());

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn secret_corrupted_file_rejected() {
    let temp_dir = unique_temp_dir();
    let config = dummy_config(temp_dir.clone());
    let secret_file = config.secret_path();

    // Write incomplete secret (< 32 bytes)
    tokio::fs::write(&secret_file, b"too_short")
        .await
        .expect("write corrupt");

    let result = Secret::load_or_create(&config).await;
    assert!(result.is_err());
    assert!(matches!(result, Err(crate::error::boot::Error::Secret(_))));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn secret_debug_redaction() {
    let raw = [7u8; 32];
    let secret = Secret::new(raw);
    let debug_str = format!("{secret:?}");

    assert!(debug_str.contains("* * *"));
    assert!(!debug_str.contains("7, 7, 7"));
    assert!(!debug_str.contains("070707"));
}
