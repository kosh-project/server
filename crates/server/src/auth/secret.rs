use crate::error::boot::{Error, Result};
use kosh_core::config::Config;
use std::{
    fmt::Debug, fs::Permissions, io::ErrorKind, os::unix::fs::PermissionsExt,
    path::Path,
};
use tokio::fs;

#[derive(Clone, Copy)]
pub struct Secret([u8; 32]);

impl Secret {
    async fn load<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let bytes = fs::read(path.as_ref()).await?;

        let key = bytes.try_into().map_err(|_| {
            Error::Secret("Corrupted secret, expected 32 bytes".to_owned())
        })?;

        Ok(Self(key))
    }

    async fn create<P>(path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let secret: [u8; 32] = rand::random();

        fs::write(path.as_ref(), secret).await?;
        fs::set_permissions(path, Permissions::from_mode(0o600)).await?;

        Ok(Self(secret))
    }

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

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0
    }

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
