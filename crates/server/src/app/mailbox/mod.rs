use std::time::Duration;

use moka::future::Cache;

mod entry;
mod error;
pub mod status;
mod token;

pub use entry::Entry;
pub use error::{Error, Result};
pub use status::Status;
pub use token::{Receipt, Token};

use crate::app::{mailbox::Error::NotFound, state::UserId};

const BASE32_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

#[derive(Clone)]
pub struct Mailbox {
    cache: Cache<String, Entry>,
}

impl Default for Mailbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Mailbox {
    pub const TTL_SECS: u64 = 300;

    #[must_use]
    pub fn new() -> Self {
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(Self::TTL_SECS))
            .max_capacity(1_000)
            .build();

        Self { cache }
    }

    fn generate_id() -> String {
        let bytes: [u8; 5] = rand::random();
        let n = u64::from_be_bytes([
            0, 0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4],
        ]);

        let mut id = String::with_capacity(8);
        for i in (0..8).rev() {
            let index = usize::try_from((n >> (i * 5)) & 0x1f).unwrap_or(0);
            if let Some(&ch) = BASE32_ALPHABET.get(index) {
                id.push(char::from(ch));
            }
        }
        id
    }

    async fn generate_unique_id(&self) -> String {
        loop {
            let id = Self::generate_id();
            if !self.cache.contains_key(&id) {
                return id;
            }
        }
    }

    /// Flow A: Receiver opens a mailbox, submits ephemeral public key, gets 8-char ID and polling token.
    pub async fn open(&self, pub_temp: Vec<u8>) -> Receipt {
        let id = self.generate_unique_id().await;
        let token = Token::new();
        let entry = Entry::new_ready(pub_temp, token.hash);
        self.cache.insert(id.clone(), entry).await;
        Receipt::new(id, token)
    }

    /// Flow B: Authenticated device offers a pairing mailbox waiting for a new device to join.
    pub async fn offer(&self, user_id: UserId) -> String {
        let id = self.generate_unique_id().await;
        let entry = Entry::new_waiting(user_id);
        self.cache.insert(id.clone(), entry).await;
        id
    }

    /// Flow B: Unauthenticated device joins offered mailbox with ephemeral public key.
    pub async fn join(&self, id: &str, pub_temp: Vec<u8>) -> Result<String> {
        let entry = self.cache.get(id).await.ok_or(NotFound)?;
        entry.join(pub_temp)
    }

    /// Flow B: Authenticated device polls status to check if receiver has joined.
    pub async fn status(
        &self,
        id: &str,
        caller_id: UserId,
    ) -> Result<Option<Vec<u8>>> {
        let entry = self.cache.get(id).await.ok_or(NotFound)?;
        entry.status(caller_id)
    }

    /// Flow A & B: Authenticated device deposits encrypted payload into the mailbox.
    pub async fn fulfill(
        &self,
        id: &str,
        caller_id: UserId,
        ciphertext: Vec<u8>,
    ) -> Result<()> {
        let entry = self.cache.get(id).await.ok_or(NotFound)?;
        entry.fulfill(caller_id, ciphertext)
    }

    /// Flow A & B: Receiver polls with polling token. On fulfillment, burns and evicts from RAM.
    pub async fn pickup(
        &self,
        id: &str,
        raw_token: &str,
    ) -> Result<Option<Vec<u8>>> {
        let entry = self.cache.get(id).await.ok_or(NotFound)?;
        let payload = entry.pickup(raw_token)?;
        if payload.is_some() {
            self.cache.remove(id).await;
        }
        Ok(payload)
    }
}
