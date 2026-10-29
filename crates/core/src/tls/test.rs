#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_join)]
#![allow(clippy::expect_used)]

use super::*;
use anyhow::Context;
use tmpdir::TmpDir;

#[tokio::test]
async fn identity_create_and_load() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls")
        .await
        .context("failed to create temporary directory")?;

    let extra_sans = ["kosh.local", "192.168.1.100"];
    let identity = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
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

    let extra_sans = ["kosh.local", "192.168.1.100"];
    let id1 = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
        .await
        .context("initial identity creation")?;

    let id2 = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
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

    let extra_sans = ["kosh.local", "192.168.1.100"];
    let identity = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
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

    let extra_sans = ["kosh.local", "192.168.1.100"];
    let id1 = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
        .await
        .context("initial")?;

    // Remove key only
    let key_path = temp_dir.to_path_buf().join("tls").join("key.pem");
    tokio::fs::remove_file(key_path)
        .await
        .context("remove key")?;

    // load_or_create should detect missing key and regenerate
    let id2 = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
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

    let extra_sans = ["kosh.local", "192.168.1.100"];
    let id1 = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
        .await
        .context("initial")?;

    // Remove cert only
    let cert_path = temp_dir.to_path_buf().join("tls").join("cert.pem");
    tokio::fs::remove_file(cert_path)
        .await
        .context("remove cert")?;

    // load_or_create should detect missing cert and regenerate
    let id2 = Identity::load_or_create(temp_dir.as_ref(), &extra_sans)
        .await
        .context("regenerated")?;
    // Test: verify identity is regenerated with a new certificate and key when cert is missing
    assert_ne!(id1.cert_pem, id2.cert_pem);
    assert_ne!(id1.key_pem, id2.key_pem);

    Ok(())
}

#[test]
fn lan_ips_returns_valid_non_loopback_ips() -> anyhow::Result<()> {
    let ips = lan_ips();
    let mut seen = std::collections::HashSet::new();

    for ip_str in &ips {
        let ip: std::net::IpAddr = ip_str
            .parse()
            .context("every LAN IP returned must parse as a valid IpAddr")?;
        assert!(
            !ip.is_loopback(),
            "LAN IP {ip_str} must not be a loopback address"
        );
        assert!(
            seen.insert(ip_str.clone()),
            "LAN IP list must not contain duplicates: {ip_str}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn identity_forge_with_duplicate_and_empty_sans() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls")
        .await
        .context("failed to create temporary directory")?;

    let dirty_sans = [
        "localhost",
        "127.0.0.1",
        "",
        "   ",
        "kosh.local",
        "kosh.local",
        "192.168.1.50",
    ];

    let identity = Identity::load_or_create(temp_dir.as_ref(), &dirty_sans)
        .await
        .context(
            "identity creation with redundant and empty SANs should succeed",
        )?;

    assert!(identity.cert_pem.contains("-----BEGIN CERTIFICATE-----"));
    assert!(identity.key_pem.contains("-----BEGIN PRIVATE KEY-----"));

    let fp = identity.fingerprint()?;
    assert_eq!(fp.len(), 64);

    let raw = identity.fingerprint_raw()?;
    assert_ne!(raw, [0u8; 32]);

    Ok(())
}

#[tokio::test]
async fn tls_init_with_mdns_enabled_and_disabled() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls-init")
        .await
        .context("failed to create temporary directory")?;

    let mut config = Config {
        vault_path: temp_dir.to_path_buf(),
        port: 23342,
        host: "0.0.0.0".to_string(),
        enable_tls: true,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: crate::config::Passkey::default(),
        mdns: crate::config::Mdns {
            enable: true,
            hostname: "kosh-init-test".to_string(),
            ..Default::default()
        },
    };

    // 1. Init with mDNS enabled
    let id1 = init(&config)
        .await
        .context("tls::init with mdns enabled should succeed")?;
    assert!(temp_dir.to_path_buf().join("tls/cert.pem").exists());
    assert!(temp_dir.to_path_buf().join("tls/key.pem").exists());

    let fp1 = id1.fingerprint()?;
    assert_eq!(fp1.len(), 64);

    // 2. Idempotent load
    let id2 = init(&config)
        .await
        .context("second tls::init should load existing identity")?;
    assert_eq!(id1.cert_pem, id2.cert_pem);
    assert_eq!(fp1, id2.fingerprint()?);

    // 3. Init with mDNS disabled in fresh dir
    let temp_dir2 = TmpDir::new("kosh-test-tls-init2")
        .await
        .context("failed to create temporary directory")?;
    config.vault_path = temp_dir2.to_path_buf();
    config.mdns.enable = false;

    let id3 = init(&config)
        .await
        .context("tls::init with mdns disabled should succeed")?;
    assert!(temp_dir2.to_path_buf().join("tls/cert.pem").exists());
    assert_eq!(id3.fingerprint()?.len(), 64);

    Ok(())
}

#[tokio::test]
async fn tls_init_fails_on_unwritable_vault_path() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-test-tls-err")
        .await
        .context("failed to create temporary directory")?;
    let file_path = temp_dir.to_path_buf().join("file_vault");
    tokio::fs::write(&file_path, "not a directory").await?;

    let config = Config {
        vault_path: file_path,
        port: 23342,
        host: "0.0.0.0".to_string(),
        enable_tls: true,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: crate::config::Passkey::default(),
        mdns: crate::config::Mdns::default(),
    };

    let result = init(&config).await;
    assert!(
        result.is_err(),
        "tls::init must fail when vault path is a file"
    );

    Ok(())
}
