use std::{
    net::{IpAddr, SocketAddr},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{ConnectInfo, FromRef, FromRequestParts},
    http::request::Parts,
    routing::head,
};
use blake3::{Hash, Hasher, keyed_hash};
use sha2::{Digest, Sha256};

use crate::{
    api::{
        self,
        Error::{self, BadRequest, Unauthorized},
    },
    app,
    auth::Secret,
    error::internal,
};

pub struct HashCash {
    pub identity_hash: String,
}

#[allow(clippy::string_slice)]
impl HashCash {
    fn verify_stateless(
        header: &str,
        ip: IpAddr,
        secret: &Secret,
    ) -> api::Result<Self> {
        let len = header.len();

        if len != 165 && len != 168 {
            return Err(Error::BadRequest("Invalid Hashcash length".into()));
        }

        let hash_result = Sha256::digest(header.as_bytes());

        #[allow(clippy::indexing_slicing)]
        if hash_result[0] != 0 || hash_result[1] != 0 {
            return Err(Error::Unauthorized(
                "Insufficient Proof of Work".into(),
            ));
        }

        let identity_hash = &header[0..64];
        let timestamp_hex = &header[80..96];
        let mac_hex = &header[96..160];
        let action = &header[160..];

        let timestamp =
            u64::from_str_radix(timestamp_hex, 16).map_err(|_| {
                Error::BadRequest("Invalid timestamp format".into())
            })?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(internal::Error::from)?
            .as_secs();

        if now > timestamp + 15 {
            return Err(Error::Unauthorized("Challenge expired".into()));
        }

        if timestamp > now + 5 {
            return Err(Error::Unauthorized(
                "Challenge from the future??".into(),
            ));
        }

        let mut hasher = Hasher::new_keyed(&secret.key());

        hasher.update(timestamp_hex.as_bytes());

        match ip {
            IpAddr::V4(ip) => hasher.update(&ip.octets()),
            IpAddr::V6(ip) => hasher.update(&ip.octets()),
        };
        hasher.update(action.as_bytes());

        let expected_mac = hasher.finalize();

        let received_mac = Hash::from_hex(mac_hex)
            .map_err(|_| Error::Unauthorized("Invalid Mac format".into()))?;

        if expected_mac != received_mac {
            return Err(Error::Unauthorized(
                "Forged or stolen challenge".into(),
            ));
        }

        Ok(Self {
            identity_hash: identity_hash.to_owned(),
        })
    }
}

impl<S> FromRequestParts<S> for HashCash
where
    S: Send + Sync,
    app::State: FromRef<S>,
{
    type Rejection = api::Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let ConnectInfo(addr) =
            ConnectInfo::<SocketAddr>::from_request_parts(parts, state)
                .await
                .map_err(|_| {
                    internal::Error::Message("Missing ConnectInfo".into())
                })?;

        let header = parts
            .headers
            .get("X-Hashcash")
            .ok_or_else(|| Unauthorized("Missing X-Hashcash header".into()))?
            .to_str()
            .map_err(|_| BadRequest("Invalid Hashcash header format".into()))?;

        if header.len() > 168 {
            return Err(BadRequest("X-Hashcash header too large".into()));
        }

        let app_state = app::State::from_ref(state);
        Self::verify_stateless(header, addr.ip(), &app_state.pow_secret)
    }
}
