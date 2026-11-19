use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use bincode_next::{config, encode_to_vec};
use kosh_core::{
    config::Config,
    logger::{
        Level::Shutdown,
        Telemetry::{Heartbeat, Log},
    },
};
#[cfg(unix)]
use tokio::net::UnixDatagram;

#[cfg(not(unix))]
use tokio::net::UdpSocket;

use tokio::{
    fs::{self, File, create_dir_all},
    io::AsyncWriteExt,
    spawn,
    sync::mpsc::{Receiver, Sender, channel},
    time::{MissedTickBehavior, interval},
};

use crate::logger::{self, Entry, error::Result};

/// The number of milliseconds in one calendar day (24 * 60 * 60 * 1000).
///
/// Used to determine which daily log file an entry belongs to by comparing
/// `entry.timestamp_ms / DAY_MILLIS` against the service's `today` field.
pub(crate) const DAY_MILLIS: i64 = 86_400_000;

#[cfg(unix)]
struct Broadcaster {
    socket: UnixDatagram,
    socket_path: PathBuf,
}

#[cfg(unix)]
impl Broadcaster {
    #[allow(clippy::unused_async, clippy::unused_async_trait_impl)]
    async fn new(config: &Config) -> Result<Self> {
        let socket = UnixDatagram::unbound()?;
        Ok(Self {
            socket,
            socket_path: PathBuf::from(&config.socket_path),
        })
    }

    async fn send(&self, bytes: &[u8]) {
        let _ = self.socket.send_to(bytes, &self.socket_path).await;
    }
}

#[cfg(not(unix))]
struct Broadcaster {
    socket: UdpSocket,
    target_addr: std::net::SocketAddr,
}

#[cfg(not(unix))]
impl Broadcaster {
    async fn new(config: &Config) -> Result<Self> {
        let socket = UdpSocket::bind("127.0.0.1:0").await?;
        let target_addr = config
            .socket_path
            .parse::<std::net::SocketAddr>()
            .unwrap_or_else(|_| {
                std::net::SocketAddr::from(([127, 0, 0, 1], 23343))
            });
        Ok(Self {
            socket,
            target_addr,
        })
    }

    async fn send(&self, bytes: &[u8]) {
        let _ = self.socket.send_to(bytes, self.target_addr).await;
    }
}

/// The background logging service.
///
/// `Service` owns the receive end of the MPSC channel, the currently active log file,
/// and the broadcaster socket used for live telemetry streaming. It runs entirely on a
/// dedicated `tokio` task and never shares memory with the HTTP request threads.
///
/// Callers interact with the service indirectly through the [`crate::logger::GLOBAL_LOGGER`]
/// sender and the [`logger::Handle`] returned by [`Service::start`]. The `Service` itself is
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
    /// The absolute path to the directory where daily log files are stored.
    ///
    /// Derived from [`Config::log_path`](kosh_core::config::Config::log_path) at startup
    /// (resolving to `<vault_path>/state/logs`) and used when opening new daily files
    /// during log rotation.
    log_path: PathBuf,
    /// Cross-platform broadcaster socket for admin CLI telemetry.
    broadcaster: Broadcaster,
}

impl Service {
    /// Initializes the logging service and spawns its background task.
    ///
    /// This method must be called once during server startup. It:
    ///
    /// 1. Creates a bounded MPSC channel with capacity 1000.
    /// 2. Resolves the log directory via [`Config::log_path`](kosh_core::config::Config::log_path) and creates `<vault_path>/state/logs` if it does not exist.
    /// 3. Opens (or creates) the current day's log file in append mode.
    /// 4. Creates an unbound Unix Datagram Socket for broadcasting.
    /// 5. Spawns a dedicated `tokio` task that runs the `Service::run` loop.
    ///
    /// The returned `Sender` should be stored in [`crate::logger::GLOBAL_LOGGER`] immediately
    /// after this call. The returned [`logger::Handle`] should be kept alive and awaited
    /// during graceful shutdown via [`logger::Handle::shutdown_with_grace`].
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration specifying log directories and Unix socket paths.
    ///
    /// # Errors
    ///
    /// Returns [`crate::logger::Error::Io`] if the log directory cannot be created or the initial log file
    /// cannot be opened.
    pub async fn start(
        config: &Config,
    ) -> Result<(Sender<Entry>, logger::Handle)> {
        let (sender, receiver) = channel(1000);

        let log_path = config.log_path();

        create_dir_all(&log_path).await?;

        let time = chrono::Utc::now().timestamp_millis();

        let active_file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&log_path.join(format_date_time(time)))
            .await?;

        let broadcaster = Broadcaster::new(config).await?;

        let service = Self {
            receiver,
            log_path,
            today: time / DAY_MILLIS,
            active_file,
            broadcaster,
        };

        let task = spawn(async move { service.run().await });
        Ok((sender, logger::Handle(task)))
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

        if let Ok(bytes) = encode_to_vec(Log(entry), config::standard()) {
            self.broadcaster.send(&bytes).await;
        }

        Ok(())
    }

    /// Broadcasts an asynchronous heartbeat pulse to the admin CLI socket.
    async fn send_hearbeat(&self) {
        let Ok(bytes) = encode_to_vec(Heartbeat, config::standard()) else {
            return;
        };

        self.broadcaster.send(bytes.as_ref()).await;
    }
}

pub use kosh_core::logger::format_date_time;
