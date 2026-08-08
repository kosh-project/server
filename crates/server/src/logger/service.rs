use std::path::PathBuf;

use bincode_next::{config, encode_into_slice, encode_to_vec};
use chrono::{DateTime, Datelike, Utc};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
    net::UnixDatagram,
    spawn,
    sync::mpsc::{Receiver, Sender, UnboundedSender, channel},
    task::JoinHandle,
};

use crate::logger::{Entry, error::Result};

pub static SOCKET_ADDR: &str = "/tmp/kosh-cli.sock";
static DAY_MILLIS: i64 = 86_400_000;

struct Service {
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

        let log_path: PathBuf = "test/logs/".into();
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
            if let Err(e) = self.commit(entry).await {
                eprintln!("LOGGER WARNING: Failed to commit log: {e}");
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

            self.today = chrono::Utc::now().timestamp_millis() / DAY_MILLIS;            
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

struct LoggerHandler(JoinHandle<()>);
