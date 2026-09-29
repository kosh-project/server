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

/// The verified output of a successful Proof-of-Work challenge validation.
///
/// An instance of this type is only constructible by calling
/// [`HashCash::verify_stateless`], which validates the full `X-Hashcash`
/// header in one pass. Handlers that receive a `HashCash` extension can trust
/// that the `PoW` difficulty, timestamp freshness, IP binding, and BLAKE3 MAC
/// have all been verified before they run.
///
/// The `X-Hashcash` header has the following fixed-width layout:
///
/// ```text
/// Offset   Length   Field
/// 0        64       identity_hash  (hex-encoded SHA-256 of the user's public identity)
/// 64       16       nonce          (arbitrary 64-bit value chosen by the client)
/// 80       16       timestamp_hex  (Unix seconds, hex-encoded, from the server challenge)
/// 96       64       mac_hex        (BLAKE3 keyed-hash of timestamp + IP + action)
/// 160      5 or 8   action         ("login" or "register")
/// ```
///
/// Total length is exactly 165 bytes for `login` and 168 bytes for `register`.
#[derive(Clone)]
pub struct HashCash {
    /// The hex-encoded identity hash extracted and verified from the header.
    ///
    /// This is the `SHA-256` hash of the user's public identity key, as
    /// supplied by the client. It is safe to decode with `hex::decode` because
    /// it occupies a fixed 64-character ASCII region of the header.
    pub identity_hash: String,
}

#[allow(clippy::string_slice)]
impl HashCash {
    /// Validates a raw `X-Hashcash` header string and returns a verified [`HashCash`].
    ///
    /// Validation is performed in the following order, from cheapest to most expensive:
    ///
    /// 1. **Length check** — Rejects the header if it is not exactly 165 or 168 bytes.
    /// 2. **Proof-of-Work** — Computes one SHA-256 hash; rejects if the first two bytes
    ///    are not both zero (i.e., fewer than 16 leading zero bits).
    /// 3. **Action validation** — Rejects any action that is not `"login"` or `"register"`.
    /// 4. **Timestamp** — Rejects challenges older than 15 seconds or more than 5 seconds
    ///    in the future (to accommodate minor NTP drift).
    /// 5. **BLAKE3 MAC** — Recomputes the MAC over `(timestamp, client_ip, action)` and
    ///    rejects the header if it does not match, proving the challenge was genuinely
    ///    issued by this server to this IP address.
    ///
    /// # Errors
    ///
    /// - `400 Bad Request` — Header length is invalid or the timestamp is not valid hex.
    /// - `401 Unauthorized` — `PoW` difficulty is insufficient, the challenge has expired,
    ///   the timestamp is too far in the future, or the MAC does not match.
    pub fn verify_stateless(
        header: &str,
        ip: IpAddr,
        secret: &Secret,
    ) -> Result<Self> {
        Self::verify_len(header.len())?;
        Self::verify_pow(header)?;

        // All field offsets are deterministic because every field before `action`
        // has a fixed width. No delimiter parsing is needed.
        let identity_hash = &header[0..64];
        let timestamp_hex = &header[80..96];
        let mac_hex = &header[96..160];
        let action = &header[160..];

        // Validate action before the more expensive timestamp and MAC checks.
        if action != "login" && action != "register" {
            return Err(BadRequest("Invalid action in challenge"));
        }

        Self::verify_timestamp(timestamp_hex)?;
        Self::verify_mac(timestamp_hex, action, mac_hex, ip, secret)?;

        Ok(Self {
            identity_hash: identity_hash.to_owned(),
        })
    }

    /// Rejects the header if its total byte length is not 165 (`login`) or 168 (`register`).
    #[inline]
    const fn verify_len(len: usize) -> Result<()> {
        if len != 165 && len != 168 {
            Err(BadRequest("Invalid X-Hashcash header length"))
        } else {
            Ok(())
        }
    }

    /// Computes SHA-256 over the entire header and rejects it if the first two
    /// bytes are not both zero (fewer than 16 leading zero bits).
    #[inline]
    #[allow(clippy::indexing_slicing)]
    fn verify_pow(header: &str) -> Result<()> {
        let hash = Sha256::digest(header.as_bytes());

        if hash[0] != 0 || hash[1] != 0 {
            Err(Unauthorized("Insufficient Proof of Work"))
        } else {
            Ok(())
        }
    }

    /// Parses the hex-encoded timestamp and rejects it if the challenge has
    /// expired (older than 15 seconds) or is implausibly far in the future
    /// (more than 5 seconds ahead, allowing for minor NTP skew).
    #[inline]
    fn verify_timestamp(timestamp_hex: &str) -> Result<()> {
        let timestamp = u64::from_str_radix(timestamp_hex, 16)
            .map_err(|_| BadRequest("Invalid timestamp format"))?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(internal::Error::from)?
            .as_secs();

        if now > timestamp + 15 {
            return Err(Unauthorized("Challenge expired"));
        }
        if timestamp > now + 5 {
            return Err(Unauthorized("Challenge timestamp is in the future"));
        }

        Ok(())
    }

    /// Recomputes the BLAKE3 MAC over `(timestamp_hex, client_ip_octets, action)` and
    /// compares it to the MAC encoded in the header.
    ///
    /// Using raw IP octets instead of a formatted string avoids a heap
    /// allocation and produces an unambiguous byte sequence regardless of
    /// whether the client is on IPv4 or IPv6.
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

        let received_mac = Hash::from_hex(mac_hex)
            .map_err(|_| Unauthorized("Invalid MAC format"))?;

        if expected_mac != received_mac {
            return Err(Unauthorized("Forged or stolen challenge"));
        }

        Ok(())
    }
}
