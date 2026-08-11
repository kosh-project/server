use std::{path::PathBuf, time::Duration};

use bincode_next::{config, encode_to_vec};
use chrono::{DateTime, Datelike, Utc};
use tokio::{
    fs::{self, File, create_dir_all},
    io::AsyncWriteExt,
    net::UnixDatagram,
    spawn,
    sync::mpsc::{Receiver, Sender, channel},
    task::JoinHandle,
    time::timeout,
};

use crate::{
    fatal,
    logger::{
        Entry, Level, Module,
        error::{Error::LogDirectoryInitialization, Result},
    },
};

pub static SOCKET_ADDR: &str = "/tmp/kosh-cli.sock";
static DAY_MILLIS: i64 = 86_400_000;

pub struct Service {
    receiver: Receiver<Entry>,
    active_file: File,
    today: i64,
    log_path: PathBuf,
    socket: UnixDatagram,
}

impl Service {
    pub async fn start(
        capacity: usize,
    ) -> Result<(Sender<Entry>, LoggerHandler)> {
        let (sender, receiver) = channel(capacity);

        let log_path = dirs::state_dir()
            .ok_or(LogDirectoryInitialization)?
            .join("kosh")
            .join("logs");

        create_dir_all(&log_path).await?;

        let time = chrono::Utc::now().timestamp_millis();

        let active_file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path.join(format_date_time(time)))
            .await?;

        let socket = UnixDatagram::unbound()?;

        let service = Self {
            receiver,
            log_path,
            today: time / DAY_MILLIS,
            active_file,
            socket,
        };

        let task = spawn(async move { service.run().await });
        Ok((sender, LoggerHandler(task)))
    }

    async fn run(mut self) {
        while let Some(entry) = self.receiver.recv().await {
            if let Level::Shutdown = entry.level {
                return;
            };
            if let Err(e) = self.commit(entry).await {
                eprintln!("Failed to commit log : {e}");
            }
        }
    }

    async fn commit(&mut self, entry: Entry) -> Result<()> {
        if entry.timestamp_ms / DAY_MILLIS != self.today {
            self.active_file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.log_path.join(format_date_time(entry.timestamp_ms)))
                .await?;

            self.today = entry.timestamp_ms / DAY_MILLIS;
        }

        let bytes = encode_to_vec(&entry, config::standard())?;

        self.active_file.write_all(&bytes).await?;
        let _ = self.socket.send_to(&bytes, SOCKET_ADDR).await;

        Ok(())
    }
}

fn format_date_time(time_stamp_millis: i64) -> String {
    let time =
        DateTime::from_timestamp_millis(time_stamp_millis).unwrap_or_default();
    format!("log_{}-{}-{}.bin", time.year(), time.month(), time.day())
}

pub struct LoggerHandler(JoinHandle<()>);

impl LoggerHandler {
    pub async fn shutdown_with_grace(self, secs: u64) {
        if let Err(e) = timeout(Duration::from_secs(secs), self.0).await {
            fatal!(
                Module::Logger,
                "Grace period of {secs} secs, timed out, forcefully terminating engine.\n{e}"
            )
        };
    }
}

#[cfg(test)]
mod test {
    use std::{
        env::{self, remove_var, set_var, var_os},
        io::BufReader,
        path::Path,
    };

    use axum::handler;
    use chrono::Utc;
    use serial_test::serial;
    use std::fs::File;
    use tmpdir::TmpDir;
    use tokio_util::io::simplex::new;

    use crate::{log, logger};

    use super::*;

    async fn with_temp_env<F, Fut, T>(f: F) -> anyhow::Result<T>
    where
        F: FnOnce(PathBuf, Sender<Entry>, LoggerHandler) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let temp_dir = TmpDir::new("kosh-test").await?;
        let old_state_dir = var_os("XDG_STATE_HOME");
        let result;

        unsafe {
            env::set_var("XDG_STATE_HOME", temp_dir.to_path_buf());

            let (sender, log_handler) =
                logger::Service::start(1000).await.unwrap();
            result = f(temp_dir.to_path_buf(), sender, log_handler).await;

            match old_state_dir {
                Some(x) => set_var("XDG_STATE_HOME", x),
                None => remove_var("XDG_STATE_HOME"),
            }
        }
        result
    }

    #[tokio::test]
    #[serial]
    async fn logger_commits_multiple_entries_to_disk() -> anyhow::Result<()> {
        with_temp_env(|tmp_path, sender, handle| async move {
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

            let log_file = tmp_path
                .join("kosh")
                .join("logs")
                .join(format_date_time(Utc::now().timestamp_millis()));

            timeout(Duration::from_secs(3), handle.shutdown_with_grace(2))
                .await?;

            let mut reader = BufReader::new(File::open(&log_file)?);

            let entry1: Entry = bincode_next::decode_from_reader(
                &mut reader,
                config::standard(),
            )?;

            let entry2: Entry = bincode_next::decode_from_std_read(
                &mut reader,
                config::standard(),
            )?;

            assert_eq!(entry1.message, "First Entry");
            assert_eq!(entry2.message, "Second Entry");

            Ok(())
        })
        .await?;

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn broadcasting_works_via_unix_socket() -> anyhow::Result<()> {
        with_temp_env(|_, sender, handle| async move {
            let _ = fs::remove_file(SOCKET_ADDR).await;
            let recv_socket = UnixDatagram::bind(SOCKET_ADDR)?;

            let mut buffer = [0u8; 512];

            let entry = Entry {
                module: Module::Api,
                level: Level::Error,
                message: "Bro where's socket??".into(),
                timestamp_ms: 0,
            };

            sender.send(entry.clone()).await?;

            let (len, _) = recv_socket.recv_from(&mut buffer).await?;

            let entry: Entry = bincode_next::decode_from_slice(
                &buffer[..len],
                config::standard(),
            )?
            .0;

            sender
                .send(Entry {
                    level: Level::Shutdown,
                    ..entry
                })
                .await?;

            handle.shutdown_with_grace(2).await;

            let _ = fs::remove_file(SOCKET_ADDR).await;
            Ok(())
        })
        .await?;

        Ok(())
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn file_rotation_on_every_new_day() -> anyhow::Result<()> {
        with_temp_env(|tmp_path, sender, handle| async move {
            let time = chrono::Utc::now();
            let entry = Entry {
                level: Level::Error,
                message: "My log not My Log".into(),
                timestamp_ms: time.timestamp_millis(),
                module: Module::Api,
            };

            sender.send(entry.clone()).await?;

            let tomorrow = time.timestamp_millis() + DAY_MILLIS * 2;

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

            let log_dir = tmp_path.join("kosh").join("logs");
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
}
