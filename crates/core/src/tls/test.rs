#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]
#![allow(clippy::unnecessary_join)]

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kosh_test_tls_{nonce}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[tokio::test]
async fn identity_create_and_load() {
    let temp_dir = unique_temp_dir();
    let identity = Identity::load_or_create(&temp_dir)
        .await
        .expect("identity creation should succeed");

    let cert_path = temp_dir.join("tls").join("cert.pem");
    let key_path = temp_dir.join("tls").join("key.pem");

    assert!(cert_path.exists());
    assert!(key_path.exists());

    assert!(identity.cert_pem.contains("-----BEGIN CERTIFICATE-----"));
    assert!(identity.cert_pem.contains("-----END CERTIFICATE-----"));
    assert!(identity.key_pem.contains("-----BEGIN PRIVATE KEY-----"));
    assert!(identity.key_pem.contains("-----END PRIVATE KEY-----"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn identity_load_idempotent() {
    let temp_dir = unique_temp_dir();

    let id1 = Identity::load_or_create(&temp_dir)
        .await
        .expect("initial identity creation");

    let id2 = Identity::load_or_create(&temp_dir)
        .await
        .expect("subsequent identity load");

    assert_eq!(id1.cert_pem, id2.cert_pem);
    assert_eq!(id1.key_pem, id2.key_pem);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn fingerprint_verification() {
    let temp_dir = unique_temp_dir();
    let identity = Identity::load_or_create(&temp_dir)
        .await
        .expect("identity creation");

    let hex_fingerprint = identity.fingerprint().expect("hex fingerprint");
    assert_eq!(hex_fingerprint.len(), 64);
    assert!(hex_fingerprint.chars().all(|c| c.is_ascii_hexdigit()));

    let raw_fingerprint = identity.fingerprint_raw().expect("raw fingerprint");
    assert_eq!(raw_fingerprint.len(), 32);

    let computed_hex = raw_fingerprint
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("");

    assert_eq!(hex_fingerprint, computed_hex);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn orphan_key_recovery() {
    let temp_dir = unique_temp_dir();
    let id1 = Identity::load_or_create(&temp_dir).await.expect("initial");

    // Remove key only
    let key_path = temp_dir.join("tls").join("key.pem");
    tokio::fs::remove_file(key_path).await.expect("remove key");

    // load_or_create should detect missing key and regenerate
    let id2 = Identity::load_or_create(&temp_dir)
        .await
        .expect("regenerated");
    assert_ne!(id1.cert_pem, id2.cert_pem);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn orphan_cert_recovery() {
    let temp_dir = unique_temp_dir();
    let id1 = Identity::load_or_create(&temp_dir).await.expect("initial");

    // Remove cert only
    let cert_path = temp_dir.join("tls").join("cert.pem");
    tokio::fs::remove_file(cert_path).await.expect("remove cert");

    // load_or_create should detect missing cert and regenerate
    let id2 = Identity::load_or_create(&temp_dir)
        .await
        .expect("regenerated");
    assert_ne!(id1.cert_pem, id2.cert_pem);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
