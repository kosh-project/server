#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
use kosh_core::config::{Config, Mdns, Passkey};
use kosh_core::tls;
use tmpdir::TmpDir;
use webdav_server::{
    auth::Secret,
    logger,
    mdns,
    model::database,
    storage,
};

#[tokio::test]
async fn test_server_boot_helpers_pipeline() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-boot-e2e")
        .await
        .context("create temp dir")?;
    let vault_path = temp_dir.to_path_buf().join("vault");

    let config = Config {
        vault_path: vault_path.clone(),
        port: 23342,
        host: "127.0.0.1".to_string(),
        enable_tls: true,
        socket_path: temp_dir
            .to_path_buf()
            .join("kosh.sock")
            .to_string_lossy()
            .to_string(),
        passkey: Passkey::default(),
        mdns: Mdns {
            enable: true,
            hostname: "kosh-boot-test".to_string(),
            ..Default::default()
        },
    };

    // 1. Logger initialization & status check
    assert!(!logger::logging_enabled());
    let logger_handle = logger::init(&config).await.context("logger::init")?;
    assert!(logger::logging_enabled());

    // Calling logger::init again must return AlreadyInitiated error
    let reinit = logger::init(&config).await;
    assert!(matches!(
        reinit,
        Err(webdav_server::error::boot::Error::AlreadyInitiated(_))
    ));

    // 2. Storage initialization: creates vault root directory
    storage::init(&config).await.context("storage::init")?;
    assert!(vault_path.exists());

    // 3. Database initialization: connects and applies migrations
    let pool = database::init(&config).await.context("database::init")?;
    let db_path = vault_path.join("metadata.db");
    assert!(db_path.exists());

    // Verify migrations by querying sqlite
    let (user_count,): (i64,) = sqlx::query_as("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .context("query users")?;
    assert_eq!(user_count, 0);

    // 4. Secret initialization: loads or creates server secret
    let _secret = Secret::load_or_create(&config).await.context("secret load/create")?;
    assert!(config.secret_path().exists());

    // 5. TLS initialization: creates certificate with SANs
    let identity = tls::init(&config).await.context("tls::init")?;
    assert!(vault_path.join("tls/cert.pem").exists());
    assert!(vault_path.join("tls/key.pem").exists());
    assert_eq!(identity.fingerprint()?.len(), 64);

    // 6. mDNS initialization: starts broadcaster
    let broadcaster = mdns::init(&config);
    assert!(broadcaster.is_some());

    // 7. Clean teardown
    if let Some(b) = broadcaster {
        b.shutdown();
    }
    pool.close().await;

    webdav_server::shutdown!("Boot pipeline test teardown");
    logger_handle.shutdown_with_grace(2).await;

    Ok(())
}
