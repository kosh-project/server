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
    assert_eq!(config.port, 6969);
    assert_eq!(config.host, "0.0.0.0");
    assert!(!config.enable_tls);
    assert_eq!(config.socket_path, "/tmp/kosh.sock");

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
