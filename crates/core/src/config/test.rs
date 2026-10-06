#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;
use serial_test::serial;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("kosh_test_config_{nonce}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn default_vault_path_is_relative() {
    let path = default_vault_path();
    assert_eq!(path, PathBuf::from("./vault"));
}

#[tokio::test]
#[serial]
async fn config_path_precedence_env() {
    let temp_dir = unique_temp_dir();
    let custom_path = temp_dir.join("custom_config.kdl");

    // Safety: Test executed serially with exclusive environment access.
    unsafe {
        std::env::set_var("KOSH_CONFIG", &custom_path);
    }

    let resolved = Config::path();
    assert_eq!(resolved, custom_path);

    unsafe {
        std::env::remove_var("KOSH_CONFIG");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
#[serial]
async fn load_or_init_creates_fresh_file_and_loads() {
    let temp_dir = unique_temp_dir();
    let config_path = temp_dir.join("subfolder").join("config.kdl");

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    assert!(!config_path.exists());

    let config = Config::load_or_init().await.expect("Failed to init config");

    assert!(config_path.exists());
    assert_eq!(config.port, 6969);
    assert_eq!(config.host, "0.0.0.0");
    assert!(!config.enable_tls);
    assert_eq!(config.socket_path, "/tmp/kosh.sock");

    // Relative vault path should be normalized to parent directory
    let expected_vault = config_path.parent().unwrap().join("./vault");
    assert_eq!(config.vault_path, expected_vault);

    unsafe {
        std::env::remove_var("KOSH_CONFIG");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
#[serial]
async fn load_or_init_idempotent() {
    let temp_dir = unique_temp_dir();
    let config_path = temp_dir.join("config.kdl");

    let custom_kdl = r#"
vault-path "/var/kosh/custom_vault"
port 8080
host "127.0.0.1"
enable-tls true
socket-path "/run/kosh/custom.sock"
"#;

    tokio::fs::write(&config_path, custom_kdl)
        .await
        .expect("write custom config");

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let config = Config::load_or_init()
        .await
        .expect("Failed to load existing");

    assert_eq!(config.vault_path, PathBuf::from("/var/kosh/custom_vault"));
    assert_eq!(config.port, 8080);
    assert_eq!(config.host, "127.0.0.1");
    assert!(config.enable_tls);
    assert_eq!(config.socket_path, "/run/kosh/custom.sock");

    unsafe {
        std::env::remove_var("KOSH_CONFIG");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn helper_path_accessors() {
    let config = Config {
        vault_path: PathBuf::from("/srv/kosh"),
        port: 6969,
        host: "0.0.0.0".to_string(),
        enable_tls: false,
        socket_path: "/tmp/kosh.sock".to_string(),
    };

    assert_eq!(config.log_path(), PathBuf::from("/srv/kosh/state/logs"));
    assert_eq!(config.tls_identity_path(), PathBuf::from("/srv/kosh/tls"));
    assert_eq!(
        config.secret_path(),
        PathBuf::from("/srv/kosh/server.secret")
    );
}

#[tokio::test]
#[serial]
async fn malformed_kdl_returns_error() {
    let temp_dir = unique_temp_dir();
    let config_path = temp_dir.join("malformed.kdl");

    tokio::fs::write(&config_path, "invalid { unclosed block")
        .await
        .expect("write malformed config");

    unsafe {
        std::env::set_var("KOSH_CONFIG", &config_path);
    }

    let result = Config::load_or_init().await;
    assert!(result.is_err());
    assert!(matches!(result, Err(Error::Parse(_))));

    unsafe {
        std::env::remove_var("KOSH_CONFIG");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
