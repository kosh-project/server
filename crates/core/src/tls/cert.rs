use std::{
    io::{Cursor, Error},
    path::Path,
};

use crate::tls::Error::NoCertificatesFound;

use super::error::Result;
use rcgen::generate_simple_self_signed;
use rustls_pemfile::certs;
use sha2::{Digest, Sha256};
use tokio::{fs, io};

pub struct Identity {
    pub cert_pem: String,
    pub key_pem: String,
}

impl Identity {
    pub async fn load_or_create<P>(vault_path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let tls_dir = vault_path.as_ref().join("tls");
        let cert_path = tls_dir.join("cert.pem");
        let key_path = tls_dir.join("key.pem");

        if cert_path.exists() && key_path.exists() {
            Self::load(&cert_path, &key_path).await
        } else {
            fs::create_dir_all(tls_dir).await?;
            Self::create(&cert_path, &key_path).await
        }
    }

    pub async fn load<P>(cert_path: P, key_path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let cert_pem = fs::read_to_string(cert_path.as_ref()).await?;
        let key_pem = fs::read_to_string(key_path.as_ref()).await?;

        Ok(Self { cert_pem, key_pem })
    }

    async fn create<P>(cert_path: P, key_path: P) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let identity = Self::forge()?;

        fs::write(cert_path, &identity.cert_pem).await?;
        fs::write(key_path, &identity.key_pem).await?;

        Ok(identity)
    }

    fn forge() -> Result<Self> {
        let subject_alt_name = [
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "0.0.0.0".to_string(),
        ];

        let certified_key = generate_simple_self_signed(&subject_alt_name)?;

        Ok(Self {
            cert_pem: certified_key.cert.pem(),
            key_pem: certified_key.signing_key.serialize_pem(),
        })
    }

    pub fn fingerprint(&self) -> Result<String> {
        let mut reader = Cursor::new(self.cert_pem.as_bytes());
        let mut cert_iter = certs(&mut reader);

        let der = cert_iter.next().ok_or_else(|| {
            Error::new(
                io::ErrorKind::InvalidData,
                "No certificate found in PEM",
            )
        })??;

        let mut hasher = Sha256::new();
        hasher.update(der.as_ref());
        let result = hasher.finalize();

        let hex_array: Vec<String> =
            result.iter().map(|b| format!("{b:02x}")).collect();
        Ok(hex_array.join(""))
    }

    pub fn fingerprint_raw(&self) -> Result<[u8; 32]> {
        let mut reader = Cursor::new(self.cert_pem.as_bytes());
        let mut cert_iter = rustls_pemfile::certs(&mut reader);

        let der = cert_iter.next().ok_or(NoCertificatesFound)??;

        let mut hasher = Sha256::new();
        hasher.update(der.as_ref());

        Ok(hasher.finalize().into())
    }
}
