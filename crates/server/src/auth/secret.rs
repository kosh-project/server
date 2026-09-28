use crate::error::boot::{Error, Result};
use kosh_core::config::Config;
use std::{
    fmt::Debug, fs::Permissions, io::ErrorKind, os::unix::fs::PermissionsExt,
    path::Path,
};
use tokio::fs;

/// A 32-byte cryptographic secret key used for BLAKE3 keyed-hash MACs.
///
/// Two distinct instances of this type are held in [`AppState`]:
///
/// - **`secret` (`K_server`):** Loaded from or generated into `vault/server.secret`
///   on startup. It survives reboots, keeping active sessions valid across power
///   cycles. It is used to sign and verify bearer session tokens.
///
/// - **`pow_secret` (`K_ephemeral`):** Generated fresh in RAM on every server
///   boot via [`Secret::random`]. It is used exclusively to sign 15-second
///   Hashcash `PoW` challenges. When the server restarts, all pending challenges
///   are instantly invalidated.
///
/// `Secret` deliberately does not implement `Display`, `Serialize`, or any trait
/// that would allow the key material to be inadvertently logged or serialized.
/// The `Debug` implementation prints `"* * *"` instead of the key bytes.
///
/// [`AppState`]: crate::app::State
#[derive(Clone, Copy)]
pub struct Secret([u8; 32]);

impl Secret {
    /// Wraps a known 32-byte array in a `Secret`.
    ///
    /// This is intended for use in tests where a deterministic key is required.
    /// In production, prefer [`Secret::load_or_create`] or [`Secret::random`].
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Loads a 32-byte secret from a file.
    async fn load<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let bytes = fs::read(path.as_ref()).await?;

        let key = bytes.try_into().map_err(|_| {
            Error::Secret("Corrupted secret file: expected exactly 32 bytes".to_owned())
        })?;

        Ok(Self(key))
    }

    /// Generates a new random secret and writes it to a file with `0o600` permissions.
    async fn create<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let secret: [u8; 32] = rand::random();

        fs::write(path.as_ref(), secret).await?;
        fs::set_permissions(path, Permissions::from_mode(0o600)).await?;

        Ok(Self(secret))
    }

    /// Loads the persistent `K_server` secret, creating it if it does not exist.
    ///
    /// If the file at `config.secret_path()` exists, it is read and its 32 bytes
    /// are returned as a `Secret`. If the file does not exist, a new cryptographically
    /// random secret is generated, written to disk with strict `0o600` POSIX
    /// permissions, and returned.
    ///
    /// # Errors
    ///
    /// - Returns [`Error::Io`] if the file exists but cannot be read, or if the
    ///   newly generated secret cannot be written to disk.
    /// - Returns [`Error::Secret`] if the file exists but does not contain exactly
    ///   32 bytes (indicating a corrupted or manually edited secret file).
    pub async fn load_or_create(config: &Config) -> Result<Self> {
        let path = config.secret_path();

        match Self::load(&path).await {
            Ok(secret) => Ok(secret),
            Err(Error::Io(e)) if e.kind() == ErrorKind::NotFound => {
                Self::create(path).await
            }
            Err(e) => Err(e),
        }
    }

    /// Generates a fresh, cryptographically random in-memory secret.
    ///
    /// This is used to create `K_ephemeral` (`pow_secret`) in [`AppState`].
    /// The returned secret is never written to disk and is lost on process exit.
    ///
    /// [`AppState`]: crate::app::State
    #[must_use]
    pub fn random() -> Self {
        Self(rand::random())
    }

    /// Returns the raw key bytes as a slice.
    ///
    /// Prefer [`Secret::key`] when passing to `blake3` functions, as it returns
    /// the fixed-size array that `blake3` expects without an extra type conversion.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Returns the raw key bytes as a fixed-size `[u8; 32]` array.
    ///
    /// This is the format expected by `blake3::keyed_hash` and
    /// `blake3::Hasher::new_keyed`. Returning by value is free because
    /// `[u8; 32]` is `Copy`.
    #[must_use]
    pub const fn key(&self) -> [u8; 32] {
        self.0
    }
}

impl Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Secret").field(&"* * *").finish()
    }
}
