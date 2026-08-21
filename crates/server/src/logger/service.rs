use std::{path::PathBuf, time::Duration};

use bincode_next::{config, encode_to_vec};
use chrono::{DateTime, Datelike};
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

/// The filesystem path of the Unix Datagram Socket used for real-time log broadcasting.
///
/// The server's background logging task sends a copy of each serialized [`Entry`] to this
/// address after writing it to disk. The admin CLI (`kosh-cli`) binds to this socket to
/// receive the live stream. If no client is bound, the `send_to` call fails silently — the
/// server deliberately ignores the error so that the absence of the CLI never affects
/// request-path performance.
pub static SOCKET_ADDR: &str = "/tmp/kosh-cli.sock";

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
/// Callers interact with the service indirectly through the [`GLOBAL_LOGGER`] sender
/// and the [`LoggerHandler`] returned by [`Service::start`]. The `Service` itself is
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
    /// Each entry is sent to [`SOCKET_ADDR`] after being written to disk. Errors are
    /// silently ignored so that the absence of the admin CLI has no impact on the server.
    socket: UnixDatagram,
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
    /// 5. Spawns a dedicated `tokio` task that runs the [`Service::run`] loop.
    ///
    /// The returned [`Sender`] should be stored in [`GLOBAL_LOGGER`] immediately after
    /// this call. The returned [`LoggerHandler`] should be kept alive and awaited during
    /// graceful shutdown via [`LoggerHandler::shutdown_with_grace`].
    ///
    /// # Errors
    ///
    /// Returns [`error::Error::LogDirectoryInitialization`] if the XDG state directory
    /// cannot be determined. Returns [`error::Error::Io`] if the log directory cannot be
    /// created or the initial log file cannot be opened.
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

    #[must_use]
    pub fn path() -> Option<PathBuf> {
        dirs::state_dir().map(|x| x.join("kosh").join("logs"))
    }

    /// The main receive loop of the logging service.
    ///
    /// Runs until a [`Level::Shutdown`] entry is received, at which point it returns
    /// and the spawned task completes, allowing [`LoggerHandler::shutdown_with_grace`]
    /// to join cleanly.
    ///
    /// Errors from [`Service::commit`] (disk write failures, serialization failures)
    /// are printed to `stderr` using `eprintln!` rather than being propagated. This
    /// ensures that a transient I/O error does not terminate the logging service.
    async fn run(mut self) {
        while let Some(entry) = self.receiver.recv().await {
            if Level::Shutdown == entry.level {
                let _ = self.commit(entry).await;
                return;
            }
            if let Err(e) = self.commit(entry).await {
                eprintln!("Failed to commit log : {e}");
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

        let bytes = encode_to_vec(&entry, config::standard())?;

        self.active_file.write_all(&bytes).await?;
        let _ = self.socket.send_to(&bytes, SOCKET_ADDR).await;

        Ok(())
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
    /// Before calling this method, the caller must send a [`Level::Shutdown`] entry
    /// through the channel (typically via the [`crate::shutdown!`] macro) to signal the
    /// service to exit its receive loop. This method then waits up to `secs` seconds for
    /// the task to join.
    ///
    /// If the task does not finish within the grace period, a [`Level::Fatal`] log entry
    /// is emitted (which will itself be silently dropped if the sender is gone) and the
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
mod test {
    use std::env::{self, remove_var, set_var, var_os};

    use super::*;
    use chrono::Utc;
    use serial_test::serial;
    use tmpdir::TmpDir;

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

            let (sender, log_handler) = Service::start(1000).await.unwrap();
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
