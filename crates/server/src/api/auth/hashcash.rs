use std::{
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use blake3::{Hash, Hasher};
use sha2::{Digest, Sha256};

use crate::{
    api::{
        Error::{BadRequest, Unauthorized},
        Result,
    },
    auth::Secret,
    error::internal,
};

#[derive(Clone)]
pub struct HashCash {
    pub identity_hash: String,
}

#[allow(clippy::string_slice)]
impl HashCash {
    pub fn verify_stateless(
        header: &str,
        ip: IpAddr,
        secret: &Secret,
    ) -> Result<Self> {
        Self::verify_len(header.len())?;
        Self::verify_pow(header)?;

        let identity_hash = &header[0..64];
        let timestamp_hex = &header[80..96];
        let mac_hex = &header[96..160];
        let action = &header[160..];

        Self::verify_timestamp(timestamp_hex)?;
        Self::verify_mac(timestamp_hex, action, mac_hex, ip, secret)?;

        Ok(Self {
            identity_hash: identity_hash.to_owned(),
        })
    }

    #[inline]
    fn verify_len(len: usize) -> Result<()> {
        if len != 165 && len != 168 {
            Err(BadRequest("Invalid Hashcash length".into()))
        } else {
            Ok(())
        }
    }

    #[inline]
    #[allow(clippy::indexing_slicing)]
    fn verify_pow(header: &str) -> Result<()> {
        let hash = Sha256::digest(header.as_bytes());

        if hash[0] != 0 || hash[1] != 0 {
            Err(Unauthorized("Insufficient Proof of Work".into()))
        } else {
            Ok(())
        }
    }

    #[inline]
    fn verify_timestamp(timestamp_hex: &str) -> Result<()> {
        let timestamp = u64::from_str_radix(timestamp_hex, 16)
            .map_err(|_| BadRequest("Invalid timestamp format".into()))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(internal::Error::from)?
            .as_secs();

        if now > timestamp + 15 {
            return Err(Unauthorized("Challenge expired".into()));
        }
        if timestamp > now + 5 {
            return Err(Unauthorized("Challenge from the future??".into()));
        }

        Ok(())
    }

    #[inline]
    fn verify_mac(
        timestamp_hex: &str,
        action: &str,
        mac_hex: &str,
        ip: IpAddr,
        secret: &Secret,
    ) -> Result<()> {
        let mut hasher = Hasher::new_keyed(&secret.key());

        hasher.update(timestamp_hex.as_bytes());

        match ip {
            IpAddr::V4(ip) => hasher.update(&ip.octets()),
            IpAddr::V6(ip) => hasher.update(&ip.octets()),
        };

        hasher.update(action.as_bytes());

        let expected_mac = hasher.finalize();
        let recieved_mac = Hash::from_hex(mac_hex)
            .map_err(|_| Unauthorized("Invalid Mac format".into()))?;

        if expected_mac != recieved_mac {
            return Err(Unauthorized("Forged or stolen challenge".into()));
        }

        Ok(())
    }
}
