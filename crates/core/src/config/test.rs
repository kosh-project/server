#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unnecessary_wraps)]

use super::*;
use anyhow::Context;
use serial_test::serial;
use tmpdir::TmpDir;

struct EnvGuard(&'static str);

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            std::env::remove_var(self.0);
        }
    }
}

#[test]
fn default_vault_path_is_relative() -> anyhow::Result<()> {
    let path = default_vault_path();
    // Test: verify default vault path is relative to current directory
    assert_eq!(path, PathBuf::from("./vault"));
    Ok(())
}

#[tokio::test]
#[serial]
async fn config_path_precedence_env() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let custom_path = temp_dir.to_path_buf().join("custom_config.kdl");

    // Safety: Test executed serially with exclusive environment access.
    unsafe {
        std::env::set_var("KOSH_CONFIG", &custom_path);
    }

    let resolved = Config::path();
    // Test: verify custom KOSH_CONFIG environment variable takes precedence
    assert_eq!(resolved, custom_path);

    Ok(())
}

#[tokio::test]
#[serial]
async fn load_or_init_creates_fresh_file_and_loads() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path =
        temp_dir.to_path_buf().join("subfolder").join("config.kdl");

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    // Test: verify config file does not exist before initialization
    assert!(!config_path.exists());

    let config = Config::load_or_init()
        .await
        .context("failed to initialize config")?;

    // Test: verify config file was created and contains expected default values
    assert!(config_path.exists());
    assert_eq!(config.port, 23342);
    assert_eq!(config.host, "0.0.0.0");
    assert!(!config.enable_tls);
    assert_eq!(config.socket_path, "/tmp/kosh.sock");
    assert_eq!(config.passkey.id, "localhost");
    assert_eq!(config.passkey.origin, "http://localhost:23342");
    assert_eq!(config.passkey.name, "Kosh Vault");

    // Test: verify relative vault path normalizes to config parent directory
    let expected_vault = config_path
        .parent()
        .context("missing parent directory")?
        .join("./vault");
    assert_eq!(config.vault_path, expected_vault);

    Ok(())
}

#[tokio::test]
#[serial]
async fn load_or_init_idempotent() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let custom_kdl = r#"
vault-path "/var/kosh/custom_vault"
port 8080
host "127.0.0.1"
enable-tls true
socket-path "/run/kosh/custom.sock"
"#;

    tokio::fs::write(&config_path, custom_kdl)
        .await
        .context("failed to write custom config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("failed to load existing config")?;

    // Test: verify custom configuration values are preserved across load
    assert_eq!(config.vault_path, PathBuf::from("/var/kosh/custom_vault"));
    assert_eq!(config.port, 8080);
    assert_eq!(config.host, "127.0.0.1");
    assert!(config.enable_tls);
    assert_eq!(config.socket_path, "/run/kosh/custom.sock");

    Ok(())
}

#[test]
fn helper_path_accessors() -> anyhow::Result<()> {
    let config = Config {
        vault_path: PathBuf::from("/srv/kosh"),
        port: 6969,
        host: "0.0.0.0".to_string(),
        enable_tls: false,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: Passkey::default(),
        mdns: Mdns::default(),
    };

    // Test: verify helper accessors construct correct sub-paths under vault
    assert_eq!(config.log_path(), PathBuf::from("/srv/kosh/state/logs"));
    assert_eq!(config.tls_identity_path(), PathBuf::from("/srv/kosh/tls"));
    assert_eq!(
        config.secret_path(),
        PathBuf::from("/srv/kosh/server.secret")
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn malformed_kdl_returns_error() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("malformed.kdl");

    tokio::fs::write(&config_path, "invalid { unclosed block")
        .await
        .context("failed to write malformed config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    // Test: verify syntax error during KDL parse returns error variant
    assert!(result.is_err());
    assert!(matches!(result, Err(Error::Parse(_))));

    Ok(())
}

#[test]
fn passkey_default_values() {
    let default_passkey = Passkey::default();
    assert_eq!(default_passkey.id, "localhost");
    assert_eq!(default_passkey.origin, "http://localhost:23342");
    assert_eq!(default_passkey.name, "Kosh Vault");
}

#[tokio::test]
#[serial]
async fn custom_passkey_config_parsing() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let custom_kdl = r#"
vault-path "/var/kosh/custom_vault"
port 8080
host "127.0.0.1"
enable-tls true
socket-path "/run/kosh/custom.sock"

passkey {
    id "vault.example.com"
    origin "https://vault.example.com"
    name "Example Vault"
}
"#;

    tokio::fs::write(&config_path, custom_kdl)
        .await
        .context("failed to write custom config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("failed to load custom config")?;

    assert_eq!(config.passkey.id, "vault.example.com");
    assert_eq!(config.passkey.origin, "https://vault.example.com");
    assert_eq!(config.passkey.name, "Example Vault");

    Ok(())
}

#[tokio::test]
#[serial]
async fn custom_passkey_config_partial_decoding() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("partial_config.kdl");

    let custom_kdl = r#"
vault-path "/var/kosh/custom_vault"
port 8080
host "127.0.0.1"
enable-tls true
socket-path "/run/kosh/custom.sock"

passkey {
    id "partial.example.com"
}
"#;

    tokio::fs::write(&config_path, custom_kdl)
        .await
        .context("failed to write custom config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("failed to load custom config")?;

    assert_eq!(config.passkey.id, "partial.example.com");
    assert_eq!(config.passkey.origin, "http://localhost:23342");
    assert_eq!(config.passkey.name, "");

    Ok(())
}

#[test]
fn mdns_default_values() {
    let default_mdns = Mdns::default();
    assert!(default_mdns.enable);
    assert_eq!(default_mdns.hostname, "kosh");
    assert_eq!(default_mdns.interfaces.len(), 0);
    assert_eq!(default_mdns.txt.properties.len(), 0);
}

#[tokio::test]
#[serial]
async fn custom_mdns_config_parsing() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let custom_kdl = r#"
vault-path "/var/kosh/custom_vault"
port 8080
host "127.0.0.1"
enable-tls true
socket-path "/run/kosh/custom.sock"

mdns {
    enable true
    hostname "vault-nas"
    interfaces "eth0" "wlan0"
    txt {
        property "environment" "homelab"
        property "owner" "tony"
    }
}
"#;

    tokio::fs::write(&config_path, custom_kdl)
        .await
        .context("failed to write custom config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("failed to load custom config")?;

    assert!(config.mdns.enable);
    assert_eq!(config.mdns.hostname, "vault-nas");
    assert_eq!(config.mdns.interfaces, vec!["eth0", "wlan0"]);
    assert_eq!(config.mdns.txt.properties.len(), 2);
    assert_eq!(config.mdns.txt.properties[0].key, "environment");
    assert_eq!(config.mdns.txt.properties[0].value, "homelab");
    assert_eq!(config.mdns.txt.properties[1].key, "owner");
    assert_eq!(config.mdns.txt.properties[1].value, "tony");

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_hostname_with_dot() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    hostname "vault.local"
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidHostname {
        hostname, reason, ..
    }) = err
    else {
        anyhow::bail!("expected InvalidHostname diagnostic, got {err:?}");
    };
    assert_eq!(hostname, "vault.local");
    assert!(reason.contains("dots"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_empty_property_key() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    txt {
        property "" "val"
    }
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidPropertyKey { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidPropertyKey diagnostic, got {err:?}");
    };
    assert!(reason.contains("empty"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_key_with_equals() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    txt {
        property "invalid=key" "val"
    }
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidPropertyKey { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidPropertyKey diagnostic, got {err:?}");
    };
    assert!(reason.contains('='));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_oversized_value() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let long_val = "a".repeat(129);
    let invalid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    txt {{
        property "test" "{long_val}"
    }}
}}
"#
    );

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::ValueTooLong { actual_len, .. }) = err
    else {
        anyhow::bail!("expected ValueTooLong diagnostic, got {err:?}");
    };
    assert_eq!(actual_len, 129);

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_payload_too_large() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let val = "a".repeat(100);
    let invalid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    txt {{
        property "prop1" "{val}"
        property "prop2" "{val}"
        property "prop3" "{val}"
        property "prop4" "{val}"
    }}
}}
"#
    );

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::PayloadTooLarge { total_len, .. }) = err
    else {
        anyhow::bail!("expected PayloadTooLarge diagnostic, got {err:?}");
    };
    assert!(total_len > 400);

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_empty_hostname() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    hostname ""
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidHostname { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidHostname diagnostic, got {err:?}");
    };
    assert!(reason.contains("empty"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_hostname_exceeding_63_chars()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let long_hostname = "a".repeat(64);
    let invalid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    hostname "{long_hostname}"
}}
"#
    );

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidHostname { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidHostname diagnostic, got {err:?}");
    };
    assert!(reason.contains("exceeds 63"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_accepts_hostname_with_exact_63_chars()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let max_hostname = "a".repeat(63);
    let valid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    hostname "{max_hostname}"
}}
"#
    );

    tokio::fs::write(&config_path, valid_kdl)
        .await
        .context("failed to write valid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init().await.context("should succeed")?;
    assert_eq!(config.mdns.hostname, max_hostname);

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_hostname_with_leading_or_trailing_hyphen()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    for bad_name in ["-leading-hyphen", "trailing-hyphen-"] {
        let invalid_kdl = format!(
            r#"
vault-path "./vault"
mdns {{
    hostname "{bad_name}"
}}
"#
        );

        tokio::fs::write(&config_path, invalid_kdl)
            .await
            .context("failed to write invalid config")?;

        unsafe {
            std::env::set_var("KOSH_CONFIG", &config_path);
        }

        let result = Config::load_or_init().await;
        assert!(result.is_err(), "expected error for hostname: {bad_name}");
        let err = result.err().context("expected error")?;
        let Error::Mdns(MdnsDiagnostic::InvalidHostname { reason, .. }) = err
        else {
            anyhow::bail!("expected InvalidHostname diagnostic, got {err:?}");
        };
        assert!(reason.contains("hyphen"));
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_hostname_with_invalid_characters()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    for bad_name in ["kosh_vault", "kosh server", "kosh@home", "vault!"] {
        let invalid_kdl = format!(
            r#"
vault-path "./vault"
mdns {{
    hostname "{bad_name}"
}}
"#
        );

        tokio::fs::write(&config_path, invalid_kdl)
            .await
            .context("failed to write invalid config")?;

        unsafe {
            std::env::set_var("KOSH_CONFIG", &config_path);
        }

        let result = Config::load_or_init().await;
        assert!(result.is_err(), "expected error for hostname: {bad_name}");
        let err = result.err().context("expected error")?;
        let Error::Mdns(MdnsDiagnostic::InvalidHostname { reason, .. }) = err
        else {
            anyhow::bail!("expected InvalidHostname diagnostic, got {err:?}");
        };
        assert!(reason.contains("invalid characters"));
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_empty_interface() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    interfaces "eth0" ""
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidInterface { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidInterface diagnostic, got {err:?}");
    };
    assert!(reason.contains("empty"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_interface_exceeding_15_bytes()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    interfaces "eth0123456789abc"
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidInterface { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidInterface diagnostic, got {err:?}");
    };
    assert!(reason.contains("exceeds 15 bytes"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_interface_with_invalid_chars()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    interfaces "eth 0"
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidInterface { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidInterface diagnostic, got {err:?}");
    };
    assert!(reason.contains("invalid characters"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_key_exceeding_32_chars() -> anyhow::Result<()>
{
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let long_key = "k".repeat(33);
    let invalid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    txt {{
        property "{long_key}" "val"
    }}
}}
"#
    );

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidPropertyKey { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidPropertyKey diagnostic, got {err:?}");
    };
    assert!(reason.contains("exceeds 32"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_rejects_key_with_non_printable_ascii()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let invalid_kdl = r#"
vault-path "./vault"
mdns {
    txt {
        property "bad	key" "val"
    }
}
"#;

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::InvalidPropertyKey { reason, .. }) = err
    else {
        anyhow::bail!("expected InvalidPropertyKey diagnostic, got {err:?}");
    };
    assert!(reason.contains("non-printable"));

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_boundary_limits_succeed() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    // Exactly 32-char key, 128-char value
    let key32 = "k".repeat(32);
    let val128 = "v".repeat(128);
    let valid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    hostname "valid-hostname-123"
    interfaces "eth0" "wlan.0" "veth_test-1"
    txt {{
        property "{key32}" "{val128}"
    }}
}}
"#
    );

    tokio::fs::write(&config_path, valid_kdl)
        .await
        .context("failed to write valid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("boundary config should succeed")?;
    assert_eq!(config.mdns.hostname, "valid-hostname-123");
    assert_eq!(
        config.mdns.interfaces,
        vec!["eth0", "wlan.0", "veth_test-1"]
    );
    assert_eq!(config.mdns.txt.properties.len(), 1);
    assert_eq!(config.mdns.txt.properties[0].key, key32);
    assert_eq!(config.mdns.txt.properties[0].value, val128);

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_accepts_interface_with_exact_15_bytes()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    // Exact 15-byte interface name
    let iface_15 = "eth0123456789ab";
    assert_eq!(iface_15.len(), 15);
    let valid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    interfaces "{iface_15}"
}}
"#
    );

    tokio::fs::write(&config_path, valid_kdl)
        .await
        .context("failed to write valid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("15-byte interface should succeed")?;
    assert_eq!(config.mdns.interfaces, vec![iface_15]);

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_txt_payload_exact_400_succeeds_and_401_fails()
-> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    // 4 properties: each key is 9 bytes ("prop12345"), val is 90 bytes.
    // 9 + 90 + 1 ('=') = 100 bytes per property * 4 = 400 bytes total.
    let k9 = "prop12345";
    let v90 = "a".repeat(90);
    let valid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    txt {{
        property "{k9}" "{v90}"
        property "{k9}" "{v90}"
        property "{k9}" "{v90}"
        property "{k9}" "{v90}"
    }}
}}
"#
    );

    tokio::fs::write(&config_path, valid_kdl)
        .await
        .context("failed to write valid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("400-byte payload should succeed")?;
    assert_eq!(config.mdns.txt.properties.len(), 4);

    // Now make 4th property value 91 bytes -> total 401 bytes
    let v91 = "a".repeat(91);
    let invalid_kdl = format!(
        r#"
vault-path "./vault"
mdns {{
    txt {{
        property "{k9}" "{v90}"
        property "{k9}" "{v90}"
        property "{k9}" "{v90}"
        property "{k9}" "{v91}"
    }}
}}
"#
    );

    tokio::fs::write(&config_path, invalid_kdl)
        .await
        .context("failed to write invalid config")?;

    let result = Config::load_or_init().await;
    assert!(result.is_err(), "401-byte payload should fail");
    let err = result.err().context("expected error")?;
    let Error::Mdns(MdnsDiagnostic::PayloadTooLarge { total_len, .. }) = err
    else {
        anyhow::bail!("expected PayloadTooLarge diagnostic, got {err:?}");
    };
    assert_eq!(total_len, 401);

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_accepts_single_char_hostname() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let valid_kdl = r#"
vault-path "./vault"
mdns {
    hostname "k"
}
"#;

    tokio::fs::write(&config_path, valid_kdl)
        .await
        .context("failed to write valid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("single-character hostname should succeed")?;
    assert_eq!(config.mdns.hostname, "k");

    Ok(())
}

#[tokio::test]
#[serial]
async fn mdns_validation_accepts_empty_property_value() -> anyhow::Result<()> {
    let _guard = EnvGuard("KOSH_CONFIG");
    let temp_dir = TmpDir::new("kosh-test-config")
        .await
        .context("failed to create temporary directory")?;
    let config_path = temp_dir.to_path_buf().join("config.kdl");

    let valid_kdl = r#"
vault-path "./vault"
mdns {
    txt {
        property "valueless-key" ""
    }
}
"#;

    tokio::fs::write(&config_path, valid_kdl)
        .await
        .context("failed to write valid config")?;

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .context("empty property value should succeed")?;
    assert_eq!(config.mdns.txt.properties.len(), 1);
    assert_eq!(config.mdns.txt.properties[0].key, "valueless-key");
    assert_eq!(config.mdns.txt.properties[0].value, "");

    Ok(())
}
