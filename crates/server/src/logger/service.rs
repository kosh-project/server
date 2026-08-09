use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    time::Duration,
};

use bincode_next::{config, encode_to_vec};
use chrono::Datelike;
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
    log,
    logger::{
        Entry, Level,
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

        let today = chrono::Utc::now().timestamp_millis() / DAY_MILLIS;

        let active_file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path.join(format_date_time()))
            .await?;

        let socket = UnixDatagram::unbound()?;

        let service = Self {
            receiver,
            log_path,
            today,
            active_file,
            socket,
        };

        let task = spawn(async move { service.run().await });
        Ok((sender, LoggerHandler(task)))
    }

    async fn run(mut self) {
        while let Some(entry) = self.receiver.recv().await {
            if let Level::Shutdown = entry.level {
                log!(
                    "LOGGER",
                    "Recieved shutdown signal, killing event loop here"
                );
                return;
            };
            if let Err(e) = self.commit(entry).await {
                log!(
                    "LOGGER",
                    format!("Commit failed with Error: {e}")
                );
            }
        }
    }

    async fn commit(&mut self, entry: Entry) -> Result<()> {
        if entry.timestamp_ms / DAY_MILLIS != self.today {
            self.active_file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.log_path.join(format_date_time()))
                .await?;

            self.today =
                chrono::Utc::now().timestamp_millis() / DAY_MILLIS;
        }

        let bytes = encode_to_vec(&entry, config::standard())?;

        self.active_file.write_all(&bytes).await?;
        self.socket.send_to(&bytes, SOCKET_ADDR).await?;

        Ok(())
    }
}

fn format_date_time() -> String {
    let time = chrono::Utc::now();
    format!("log_{}-{}-{}.bin", time.year(), time.month(), time.day())
}

pub struct LoggerHandler(JoinHandle<()>);

impl LoggerHandler {
    pub async fn shutdown_with_grace(self, secs: u64) {
        let _ = timeout(Duration::from_secs(secs), self.0).await;
    }
}
