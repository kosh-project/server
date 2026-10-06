#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_join)]

use super::*;
use anyhow::Context;
use tmpdir::TmpDir;

#[tokio::test]
async fn identity_create_and_load() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls")
        .await
        .context("failed to create temporary directory")?;
    let identity = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("identity creation should succeed")?;

    let cert_path = temp_dir.to_path_buf().join("tls").join("cert.pem");
    let key_path = temp_dir.to_path_buf().join("tls").join("key.pem");

    // Test: verify TLS certificate and key files are generated on disk
    assert!(cert_path.exists());
    assert!(key_path.exists());

    // Test: verify certificate and key contain expected PEM boundaries
    assert!(identity.cert_pem.contains("-----BEGIN CERTIFICATE-----"));
    assert!(identity.cert_pem.contains("-----END CERTIFICATE-----"));
    assert!(identity.key_pem.contains("-----BEGIN PRIVATE KEY-----"));
    assert!(identity.key_pem.contains("-----END PRIVATE KEY-----"));

    Ok(())
}

#[tokio::test]
async fn identity_load_idempotent() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls")
        .await
        .context("failed to create temporary directory")?;

    let id1 = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("initial identity creation")?;

    let id2 = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("subsequent identity load")?;

    // Test: verify subsequent load reuses existing certificate and key unchanged
    assert_eq!(id1.cert_pem, id2.cert_pem);
    assert_eq!(id1.key_pem, id2.key_pem);

    Ok(())
}

#[tokio::test]
async fn fingerprint_verification() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls")
        .await
        .context("failed to create temporary directory")?;
    let identity = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("identity creation")?;

    let hex_fingerprint = identity.fingerprint().context("hex fingerprint")?;
    // Test: verify hex fingerprint is a 64-character hex string
    assert_eq!(hex_fingerprint.len(), 64);
    assert!(hex_fingerprint.chars().all(|c| c.is_ascii_hexdigit()));

    let raw_fingerprint =
        identity.fingerprint_raw().context("raw fingerprint")?;
    // Test: verify raw fingerprint is 32 bytes (SHA-256)
    assert_eq!(raw_fingerprint.len(), 32);

    let computed_hex = raw_fingerprint
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("");

    // Test: verify hex fingerprint matches formatted raw SHA-256 bytes
    assert_eq!(hex_fingerprint, computed_hex);

    Ok(())
}

#[tokio::test]
async fn orphan_key_recovery() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls")
        .await
        .context("failed to create temporary directory")?;
    let id1 = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("initial")?;

    // Remove key only
    let key_path = temp_dir.to_path_buf().join("tls").join("key.pem");
    tokio::fs::remove_file(key_path)
        .await
        .context("remove key")?;

    // load_or_create should detect missing key and regenerate
    let id2 = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("regenerated")?;
    // Test: verify identity is regenerated with a new certificate and key when key is missing
    assert_ne!(id1.cert_pem, id2.cert_pem);
    assert_ne!(id1.key_pem, id2.key_pem);

    Ok(())
}

#[tokio::test]
async fn orphan_cert_recovery() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls")
        .await
        .context("failed to create temporary directory")?;
    let id1 = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("initial")?;

    // Remove cert only
    let cert_path = temp_dir.to_path_buf().join("tls").join("cert.pem");
    tokio::fs::remove_file(cert_path)
        .await
        .context("remove cert")?;

    // load_or_create should detect missing cert and regenerate
    let id2 = Identity::load_or_create(temp_dir.as_ref())
        .await
        .context("regenerated")?;
    // Test: verify identity is regenerated with a new certificate and key when cert is missing
    assert_ne!(id1.cert_pem, id2.cert_pem);
    assert_ne!(id1.key_pem, id2.key_pem);

    Ok(())
}
