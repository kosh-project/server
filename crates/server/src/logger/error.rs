/// Errors that can occur inside the logging service itself.
///
/// These errors are intentionally kept separate from the application's domain errors
/// to avoid any risk of a logging failure triggering another log attempt, which could
/// cause an infinite error loop. The `Service` handles these errors by printing directly
/// to `stderr` using `eprintln!` rather than sending them through the channel.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// A filesystem I/O error occurred while reading or writing a log file.
    ///
    /// This covers both failures to open the daily `.bin` file and failures to
    /// write serialized bytes to it. The inner error provides the OS-level detail.
    #[error("Failed to write to log file: {}", .0)]
    Io(#[from] std::io::Error),

    /// Serialization of a log entry to the `bincode` wire format failed.
    ///
    /// In practice this should never occur because `Entry` is a simple struct with
    /// well-defined field types. If it does, it indicates a bug in the `bincode`
    /// encoding configuration.
    #[error("Failed to serialzie log event: {}", .0)]
    Serialization(#[from] bincode_next::error::EncodeError),

    /// The XDG state directory could not be determined at startup.
    ///
    /// The logging service uses `dirs::state_dir()` (typically `~/.local/state`) to
    /// locate the `kosh/logs` directory. This error is returned if the OS cannot
    /// provide a valid base path, which may happen on systems without `$HOME` set or
    /// in minimal container environments.
    #[error("Failed to initialize log directory")]
    LogDirectoryInitialization,
}

pub type Result<T> = std::result::Result<T, Error>;
