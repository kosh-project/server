#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;
use bincode_next::config;
use chrono::Utc;
use kosh_core::{config::Config, logger::Level};
use std::time::Duration;
use tmpdir::TmpDir;
use tokio::{fs, net::UnixDatagram, time::timeout};

async fn with_temp_env<F, Fut, T>(f: F) -> anyhow::Result<T>
where
    F: FnOnce(Config, Sender<Entry>, service::LoggerHandler) -> Fut,
    Fut: Future<Output = anyhow::Result<T>>,
{
    let temp_dir = TmpDir::new("kosh-test").await?;

    let config = Config {
        vault_path: temp_dir.to_path_buf(),
        port: 0,
        host: "127.0.0.1".to_string(),
        enable_tls: false,
        socket_path: temp_dir
            .to_path_buf()
            .join("kosh.sock")
            .to_string_lossy()
            .into(),
    };

    let (sender, log_handler) = Service::start(&config).await.unwrap();
    f(config, sender, log_handler).await
}

#[tokio::test]
async fn logger_commits_multiple_entries_to_disk() -> anyhow::Result<()> {
    with_temp_env(|config, sender, handle| async move {
        let entry = Entry {
            level: Level::Error,
            module: Module::Api,
            message: "Holy Test".into(),
            timestamp_ms: Utc::now().timestamp_millis(),
        };

        sender
            .send(Entry {
                message: "First Entry".into(),
                ..entry
            })
            .await?;

        sender
            .send(Entry {
                message: "Second Entry".into(),
                ..entry
            })
            .await?;

        sender
            .send(Entry {
                level: Level::Shutdown,
                ..entry
            })
            .await?;

        let log_file = config
            .log_path()
            .join(format_date_time(Utc::now().timestamp_millis()));

        timeout(Duration::from_secs(3), handle.shutdown_with_grace(2))
            .await?;

        let file_bytes = std::fs::read(&log_file)?;

        let (entry1, len1): (Entry, usize) =
            bincode_next::decode_from_slice(&file_bytes, config::standard())?;

        let (entry2, _): (Entry, usize) =
            bincode_next::decode_from_slice(&file_bytes[len1..], config::standard())?;

        assert_eq!(entry1.message, "First Entry");
        assert_eq!(entry2.message, "Second Entry");

        Ok(())
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn broadcasting_works_via_unix_socket() -> anyhow::Result<()> {
    with_temp_env(|config, sender, handle| async move {
        let _ = fs::remove_file(&config.socket_path).await;
        let recv_socket = UnixDatagram::bind(&config.socket_path)?;

        let mut buffer = [0u8; 512];

        let entry = Entry {
            module: Module::Api,
            level: Level::Error,
            message: "Bro where's socket??".into(),
            timestamp_ms: 0,
        };

        sender.send(entry.clone()).await?;

        let (len, _) = recv_socket.recv_from(&mut buffer).await?;

        let entry: Entry =
            bincode_next::decode_from_slice(&buffer[..len], config::standard())?.0;

        sender
            .send(Entry {
                level: Level::Shutdown,
                ..entry
            })
            .await?;

        handle.shutdown_with_grace(2).await;

        let _ = fs::remove_file(&config.socket_path).await;
        Ok(())
    })
    .await?;

    Ok(())
}

#[tokio::test]
async fn file_rotation_on_every_new_day() -> anyhow::Result<()> {
    with_temp_env(|config, sender, handle| async move {
        let time = chrono::Utc::now();
        let entry = Entry {
            level: Level::Error,
            message: "My log not My Log".into(),
            timestamp_ms: time.timestamp_millis(),
            module: Module::Api,
        };

        sender.send(entry.clone()).await?;

        let tomorrow = time.timestamp_millis() + service::DAY_MILLIS * 2;

        sender
            .send(Entry {
                timestamp_ms: tomorrow,
                ..entry.clone()
            })
            .await?;

        sender
            .send(Entry {
                level: Level::Shutdown,
                timestamp_ms: tomorrow,
                ..entry
            })
            .await?;

        handle.shutdown_with_grace(2).await;

        let log_dir = config.log_path();
        let mut file_count = 0;

        let mut read_dir = fs::read_dir(log_dir).await?;

        while let Ok(Some(_entry)) = read_dir.next_entry().await {
            file_count += 1;
        }

        assert_eq!(
            file_count, 2,
            "Expected 2 distinct log files, found {file_count}, instead"
        );

        Ok(())
    })
    .await?;

    Ok(())
}
