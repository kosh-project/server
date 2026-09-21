use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use bincode_next::{config, encode_to_vec};
use chrono::{DateTime, Datelike};
use kosh_core::{
    config::Config,
    logger::{
        Level::Shutdown,
        Telemetry::{Heartbeat, Log},
    },
};
use tokio::{
    fs::{self, File, create_dir_all},
    io::AsyncWriteExt,
    net::UnixDatagram,
    spawn,
    sync::mpsc::{Receiver, Sender, channel},
    task::JoinHandle,
    time::{MissedTickBehavior, interval, timeout},
};

use crate::{
    fatal,
    logger::{Entry, Module, error::Result},
};

/// The number of milliseconds in one calendar day (24 * 60 * 60 * 1000).
///
/// Used to determine which daily log file an entry belongs to by comparing
/// `entry.timestamp_ms / DAY_MILLIS` against the service's `today` field.
static DAY_MILLIS: i64 = 86_400_000;

/// The background logging service.
///
/// `Service` owns the receive end of the MPSC channel, the currently active log file,
/// and the unbound Unix Datagram Socket used for broadcasting. It runs entirely on a
/// dedicated `tokio` task and never shares memory with the HTTP request threads.
///
/// Callers interact with the service indirectly through the [`crate::logger::GLOBAL_LOGGER`]
/// sender and the `LoggerHandler` returned by [`Service::start`]. The `Service` itself is
/// consumed by the background task and is not accessible after startup.
pub struct Service {
    /// The receive end of the bounded MPSC channel.
    ///
    /// The service loops on this receiver, processing one [`Entry`] at a time.
    receiver: Receiver<Entry>,
    /// The currently open log file, opened in append mode.
    ///
    /// This handle is replaced atomically (at the Rust level, not the OS level) when the
    /// service detects that a new calendar day has started, implementing log file rotation.
    active_file: File,
    /// The calendar day (as `timestamp_ms / DAY_MILLIS`) of the currently active log file.
    ///
    /// Compared against each incoming entry's timestamp to detect when a day boundary
    /// has been crossed and a new log file must be opened.
    today: i64,
    /// The absolute path to the `kosh/logs` directory.
    ///
    /// Derived from `dirs::state_dir()` at startup and used when opening new daily files
    /// during log rotation.
    log_path: PathBuf,
    /// An unbound Unix Datagram Socket used to broadcast entries to the admin CLI.
    ///
    /// Unbound means the socket has no address of its own; it can only send, not receive.
    /// Each entry is sent to `socket_path` after being written to disk. Errors are
    /// silently ignored so that the absence of the admin CLI has no impact on the server.
    socket: UnixDatagram,

    /// The filesystem path of the Unix Datagram Socket that the admin CLI is bound to.
    ///
    /// Derived from [`kosh_core::config::Config::socket_path`] at startup. The server
    /// sends each serialised [`kosh_core::logger::Telemetry`] frame to this address after
    /// writing the raw [`Entry`] to disk. If no CLI process is bound to the path, the
    /// `send_to` call fails silently.
    socket_path: PathBuf,
}

impl Service {
    /// Initializes the logging service and spawns its background task.
    ///
    /// This method must be called once during server startup. It:
    ///
    /// 1. Creates a bounded MPSC channel with the specified `capacity`.
    /// 2. Resolves the XDG state directory and creates `kosh/logs` if it does not exist.
    /// 3. Opens (or creates) the current day's log file in append mode.
    /// 4. Creates an unbound Unix Datagram Socket for broadcasting.
    /// 5. Spawns a dedicated `tokio` task that runs the `Service::run` loop.
    ///
    /// The returned `Sender` should be stored in [`crate::logger::GLOBAL_LOGGER`] immediately
    /// after this call. The returned `LoggerHandler` should be kept alive and awaited
    /// during graceful shutdown via `LoggerHandler::shutdown_with_grace`.
    ///
    /// # Errors
    ///
    /// Returns `Error::LogDirectoryInitialization` if the XDG state directory
    /// cannot be determined. Returns `Error::Io` if the log directory cannot be
    /// created or the initial log file cannot be opened.
    pub async fn start(
        config: &Config,
    ) -> Result<(Sender<Entry>, LoggerHandler)> {
        let (sender, receiver) = channel(1000);

        let log_path = config.log_path();

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
            socket_path: PathBuf::from(&config.socket_path),
        };

        let task = spawn(async move { service.run().await });
        Ok((sender, LoggerHandler(task)))
    }

    /// Returns the path to the directory where daily log files are written.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.log_path
    }

    /// The main receive loop of the logging service.
    ///
    /// Drives a `tokio::select!` loop that handles two events:
    ///
    /// - A log [`Entry`] arriving on the MPSC channel is committed to disk and broadcast
    ///   to the admin CLI as a [`kosh_core::logger::Telemetry::Log`] frame. If the entry
    ///   carries [`kosh_core::logger::Level::Shutdown`], the loop breaks and the task ends.
    /// - A 3-second idle tick fires when no entries arrive, triggering a
    ///   [`kosh_core::logger::Telemetry::Heartbeat`] over the Unix socket so the admin
    ///   CLI can distinguish an idle server from an offline one.
    ///
    /// Errors from [`Service::commit`] (disk write failures, serialization failures)
    /// are printed to `stderr` using `eprintln!` rather than being propagated. This
    /// ensures that a transient I/O error does not terminate the logging service.
    async fn run(mut self) {
        let mut timer = interval(Duration::from_secs(3));
        timer.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                Some(entry) = self.receiver.recv() => {
                    let is_shutdown = entry.level == Shutdown;

                    if let Err(e) = self.commit(entry).await {
                        eprintln!("Failed to commit logs to disk : {e}");
                    }
                    if is_shutdown {
                        break;
                    }
                },
                _ = timer.tick() => { self.send_hearbeat().await; }
            }
        }
    }

    /// Serializes and persists a single log entry.
    ///
    /// Before writing, it checks whether the entry's `timestamp_ms` falls on a different
    /// calendar day than the currently open file. If so, a new daily log file is opened
    /// and `self.today` is updated. The file is identified purely by the entry's timestamp,
    /// not by the wall clock, which prevents queue-lag from placing late-night entries into
    /// the wrong file.
    ///
    /// After writing to disk, the serialized bytes are sent to the admin CLI socket.
    /// Socket errors are silently ignored.
    ///
    /// # Errors
    ///
    /// Returns an error if the new daily log file cannot be opened, if `bincode` serialization
    /// fails, or if the `write_all` call to the active file fails.
    async fn commit(&mut self, entry: Entry) -> Result<()> {
        if entry.timestamp_ms / DAY_MILLIS != self.today {
            self.active_file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.log_path.join(format_date_time(entry.timestamp_ms)))
                .await?;

            self.today = entry.timestamp_ms / DAY_MILLIS;
        }

        let disk_bytes = encode_to_vec(&entry, config::standard())?;
        self.active_file.write_all(&disk_bytes).await?;

        if let Ok(uds_bytes) = encode_to_vec(Log(entry), config::standard()) {
            let _ = self.socket.send_to(&uds_bytes, &self.socket_path).await;
        }

        Ok(())
    }

    async fn send_hearbeat(&self) {
        let Ok(bytes) = encode_to_vec(Heartbeat, config::standard()) else {
            return;
        };

        let _ = self.socket.send_to(bytes.as_ref(), &self.socket_path).await;
    }
}

/// Produces the filename for a daily log file given a Unix timestamp in milliseconds.
///
/// The filename takes the form `log_YYYY-M-D.bin`. It is derived entirely from the
/// provided timestamp rather than from `Utc::now()`, ensuring that entries processed
/// after midnight due to channel queue lag are still written to the correct file.
///
/// If the timestamp cannot be converted to a valid [`DateTime`], the function falls back
/// to the Unix epoch (1970-01-01) via `unwrap_or_default`.
#[must_use]
pub fn format_date_time(time_stamp_millis: i64) -> String {
    let time =
        DateTime::from_timestamp_millis(time_stamp_millis).unwrap_or_default();
    format!("log_{}-{}-{}.bin", time.year(), time.month(), time.day())
}

/// A handle to the background logging task.
///
/// Returned by [`Service::start`] alongside the channel sender. The caller should
/// retain this handle and use it during the server's graceful shutdown sequence to
/// ensure that all buffered log entries are flushed to disk before the process exits.
pub struct LoggerHandler(JoinHandle<()>);

impl LoggerHandler {
    /// Waits for the logging task to finish, with a timeout.
    ///
    /// Before calling this method, the caller must send a [`kosh_core::logger::Level::Shutdown`]
    /// entry through the channel (typically via the [`crate::shutdown!`] macro) to signal the
    /// service to exit its receive loop. This method then waits up to `secs` seconds for
    /// the task to join.
    ///
    /// If the task does not finish within the grace period, a [`kosh_core::logger::Level::Fatal`]
    /// log entry is emitted (which will itself be silently dropped if the sender is gone) and the
    /// method returns, allowing the OS to clean up the task.
    pub async fn shutdown_with_grace(self, secs: u64) {
        if let Err(e) = timeout(Duration::from_secs(secs), self.0).await {
            fatal!(
                Module::Logger,
                "Grace period of {secs} secs, timed out, forcefully terminating engine.\n{e}"
            );
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::panic_in_result_fn,
    clippy::unwrap_used,
    clippy::indexing_slicing
)]
mod test {

    use super::*;
    use chrono::Utc;
    use kosh_core::logger::Level;
    use tmpdir::TmpDir;

    async fn with_temp_env<F, Fut, T>(f: F) -> anyhow::Result<T>
    where
        F: FnOnce(Config, Sender<Entry>, LoggerHandler) -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        let temp_dir = TmpDir::new("kosh-test").await?;
        let result;

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
        result = f(config, sender, log_handler).await;

        result
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
                bincode_next::decode_from_slice(
                    &file_bytes,
                    config::standard(),
                )?;

            let (entry2, _): (Entry, usize) = bincode_next::decode_from_slice(
                &file_bytes[len1..],
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
}
