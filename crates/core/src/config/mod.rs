//! KDL-based server configuration.
//!
//! The configuration file uses [KDL](https://kdl.dev) syntax and is parsed via the
//! `knuffel` crate, which provides rich error messages with source location context.
//!
//! On first run, [`Config::load_or_init`] writes a default configuration file to the
//! specified path so that operators can start immediately without manual setup.
//!
//! # Example configuration file
//!
//! ```kdl
//! vault-path "./vault"
//! host "0.0.0.0"
//! port 6969
//! enable-tls false
//! socket-path "/tmp/kosh.sock"
//! ```

mod error;

pub use error::{Error, Result};

use std::path::{Path, PathBuf};
use tokio::fs;

/// The runtime configuration for the Kosh server.
///
/// All fields have defaults that produce a working server on a fresh install.
/// The defaults can be overridden by editing the KDL configuration file.
#[derive(knuffel::Decode, Debug)]
pub struct Config {
    /// Root directory where all server data is stored.
    ///
    /// This directory will contain:
    /// - `tls/` — the self-signed certificate and private key.
    /// - `state/logs/` — daily structured log files.
    /// - `ledgers/` — per-user delta sync segments.
    /// - The blob store (content-addressable files named by their BLAKE3 hash).
    ///
    /// Defaults to `./vault` relative to the working directory.
    #[knuffel(child, unwrap(argument), default = default_vault_path())]
    pub vault_path: PathBuf,

    /// TCP port the server listens on. Defaults to `6969`.
    #[knuffel(child, unwrap(argument), default = 6969)]
    pub port: u16,

    /// IP address or hostname the server binds to. Defaults to `"0.0.0.0"` (all interfaces).
    #[knuffel(child, unwrap(argument), default = "0.0.0.0".to_string())]
    pub host: String,

    /// Whether to serve HTTPS instead of plain HTTP.
    ///
    /// When `true`, the server loads the TLS identity from `<vault_path>/tls/` and uses
    /// `rustls` for all connections. When `false`, a plain TCP listener is used.
    /// Defaults to `false`.
    #[knuffel(child, unwrap(argument), default = false)]
    pub enable_tls: bool,

    /// Filesystem path of the Unix Datagram Socket the admin CLI binds to.
    ///
    /// The server's logger background task sends structured telemetry to this path after
    /// writing each log entry to disk. The admin CLI (`kosh-cli`) binds a socket at this
    /// path to receive the live log stream. If the CLI is not running, send errors are
    /// silently ignored — the absence of the CLI never affects server performance.
    ///
    /// Defaults to `"/tmp/kosh.sock"`.
    #[knuffel(child, unwrap(argument), default = "/tmp/kosh.sock".to_string())]
    pub socket_path: String,
}

fn default_vault_path() -> PathBuf {
    PathBuf::from("./vault")
}

impl Config {
    /// Loads the configuration from `path`, creating a default file if it does not exist.
    ///
    /// If the file does not exist, a default KDL configuration is written to `path` before
    /// parsing. Any intermediate directories are created automatically.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] if the config file or its parent directories cannot be created
    /// or read. Returns [`Error::Parse`] if the KDL content cannot be decoded into a
    /// valid `Config` struct.
    pub async fn load_or_init<P>(path: &P) -> error::Result<Self>
    where
        P: AsRef<Path> + Send + Sync,
    {
        let path = path.as_ref();
        if !path.exists() {
            let default_kdl = r#"// Kosh Server Configuration

// The root directory where all blobs, databases, and keys are stored.
vault-path "./vault"

// Network bindings
host "0.0.0.0"
port 6969

// Security
enable-tls false

// Admin CLI socket
socket-path "/tmp/kosh.sock"
"#;

            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent).await?;
            }

            fs::write(path, default_kdl).await?;
        }

        let config_txt = fs::read_to_string(path).await?;
        let config = knuffel::parse(&path.to_string_lossy(), &config_txt)?;
        Ok(config)
    }

    #[must_use]
    pub fn log_path(&self) -> PathBuf {
        self.vault_path.join("state").join("logs")
    }

    #[must_use]
    pub fn tls_identity_path(&self) -> PathBuf {
        self.vault_path.join("tls")
    }
}
