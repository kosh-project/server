use std::{
    mem::replace,
    sync::{Arc, Mutex},
    time::Duration,
};

use moka::future::Cache;

use crate::{
    api::{
        self,
        Error::{BadRequest, NotFound},
    },
    app::state::UserId,
    error::internal::Error as InternalErr,
};

// Crockford's Base32
const BASE32_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Pending,
    Fulfilled(Vec<u8>),
    Consumed,
}

use Status::{Consumed, Fulfilled, Pending};

#[derive(Clone)]
pub struct Entry {
    pub user_id: UserId,
    pub status: Arc<Mutex<Status>>,
}

impl Entry {
    fn take(&self) -> api::Result<Option<Vec<u8>>> {
        let mut status = self.status.lock().map_err(|_| {
            InternalErr::Message("Mailbox mutex guard poisoned")
        })?;

        if *status == Pending {
            return Ok(None);
        }

        let old_status = replace(&mut *status, Consumed);
        drop(status);

        if let Fulfilled(payload) = old_status {
            return Ok(Some(payload));
        }

        Err(NotFound("Mailbox not found or expired"))
    }
}

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

    pub async fn create(&self, user_id: UserId) -> String {
        let mut id;
        loop {
            id = Self::generate_id();
            if !self.cache.contains_key(&id) {
                break;
            }
        }
        let entry = Entry {
            user_id,
            status: Arc::new(Mutex::new(Status::Pending)),
        };
        self.cache.insert(id.clone(), entry).await;

        id
    }

    pub async fn fulfill(
        &self,
        id: &str,
        user_id: UserId,
        payload: Vec<u8>,
    ) -> api::Result<()> {
        let entry = self.verify_id(id, user_id).await?;

        let mut status = entry.status.lock().map_err(|_| {
            InternalErr::Message("Mailbox mutex gaurd poisoned")
        })?;

        match *status {
            Pending => {
                *status = Fulfilled(payload);
                drop(status);

                Ok(())
            }
            _ => Err(BadRequest("Mailbox already fulfilled")),
        }
    }

    pub async fn verify_id(
        &self,
        id: &str,
        user_id: UserId,
    ) -> api::Result<Entry> {
        let entry = self
            .cache
            .get(id)
            .await
            .ok_or(NotFound("Mailbox not found or expired"))?;

        if entry.user_id != user_id {
            return Err(NotFound("Mailbox not found or expired"));
        }

        Ok(entry)
    }

    pub async fn pickup(
        &self,
        id: &str,
        user_id: UserId,
    ) -> api::Result<Option<Vec<u8>>> {
        let entry = self.verify_id(id, user_id).await?;

        let Some(payload) = entry.take()? else {
            return Ok(None);
        };

        self.cache.remove(id).await;

        Ok(Some(payload))
    }
}
