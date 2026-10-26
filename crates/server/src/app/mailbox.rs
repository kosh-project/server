//! In-memory ephemeral ECDH key exchange mailbox cache.
//!
//! Provides a bounded, single-use, in-memory mailbox relay with:
//! - 8-character lowercase Crockford Base32 identifiers (`0123456789abcdefghjkmnpqrstvwxyz`).
//! - Atomic state transitions: `Pending` -> `Fulfilled(Vec<u8>)` -> `Consumed`.
//! - Single-use "burn-after-reading" semantics upon pickup.
//! - Anti-enumeration security: cross-tenant access returns [`NotFound`], preventing ID discovery.
//! - Automatic expiration via bounded Time-To-Live (`TTL = 300s`) managed by [`moka::future::Cache`].

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

/// Crockford's Base32 alphabet (lowercase, avoiding ambiguous characters `i`, `l`, `o`, `u`).
const BASE32_ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Lifecycle status of an ephemeral key exchange mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Mailbox created by receiver, awaiting sender fulfillment.
    Pending,
    /// Mailbox fulfilled by sender with raw payload bytes.
    Fulfilled(Vec<u8>),
    /// Mailbox payload consumed and burned by receiver.
    Consumed,
}

use Status::{Consumed, Fulfilled, Pending};

/// Thread-safe cached mailbox entry holding user ownership and atomic lifecycle status.
#[derive(Clone)]
pub struct Entry {
    /// Owner user ID of the mailbox.
    pub user_id: UserId,
    /// Protected atomic lifecycle status.
    pub status: Arc<Mutex<Status>>,
}

impl Entry {
    /// Atomically retrieves and burns the payload if fulfilled.
    ///
    /// # Errors
    ///
    /// - Returns [`InternalErr`] if the internal mutex is poisoned.
    /// - Returns [`NotFound`] if the entry has already been consumed.
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

/// Ephemeral in-memory mailbox relay cache.
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
    /// Default time-to-live for mailbox entries in seconds (5 minutes).
    pub const TTL_SECS: u64 = 300;

    /// Constructs a new `Mailbox` cache with default capacity and TTL limits.
    #[must_use]
    pub fn new() -> Self {
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(Self::TTL_SECS))
            .max_capacity(1_000)
            .build();

        Self { cache }
    }

    /// Generates an 8-character lowercase Crockford Base32 identifier.
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

    /// Allocates a new mailbox entry bound to `user_id` and returns its 8-character ID.
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

    /// Deposits a payload into the mailbox if currently in `Pending` state.
    ///
    /// # Errors
    ///
    /// - Returns [`NotFound`] if the mailbox does not exist, has expired, belongs to another user,
    ///   or has already been consumed.
    /// - Returns [`BadRequest`] if the mailbox has already been fulfilled.
    /// - Returns [`InternalErr`] if the mailbox mutex is poisoned.
    pub async fn fulfill(
        &self,
        id: &str,
        user_id: UserId,
        payload: Vec<u8>,
    ) -> api::Result<()> {
        let entry = self.verify_id(id, user_id).await?;

        let is_consumed = {
            let mut status = entry.status.lock().map_err(|_| {
                InternalErr::Message("Mailbox mutex guard poisoned")
            })?;

            match *status {
                Pending => {
                    *status = Fulfilled(payload);
                    drop(status);
                    false
                }
                Fulfilled(_) => {
                    return Err(BadRequest("Mailbox already fulfilled"));
                }
                Consumed => true,
            }
        };

        if is_consumed {
            self.cache.remove(id).await;
            return Err(NotFound("Mailbox not found or expired"));
        }

        Ok(())
    }

    /// Verifies that a mailbox exists and belongs to the given user.
    ///
    /// To prevent mailbox enumeration attacks, returns [`NotFound`] even if the mailbox
    /// exists but belongs to a different tenant.
    ///
    /// # Errors
    ///
    /// - Returns [`NotFound`] if the mailbox does not exist, has expired, or belongs to another user.
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

    /// Retrieves the mailbox payload if fulfilled, burning the entry atomically upon pickup.
    ///
    /// Returns `Ok(None)` if the mailbox is still in `Pending` state. If fulfilled, returns
    /// `Ok(Some(payload))` and evicts the mailbox from cache.
    ///
    /// # Errors
    ///
    /// - Returns [`NotFound`] if the mailbox does not exist, has expired, belongs to another user,
    ///   or has already been consumed.
    /// - Returns [`InternalErr`] if the mailbox mutex is poisoned.
    pub async fn pickup(
        &self,
        id: &str,
        user_id: UserId,
    ) -> api::Result<Option<Vec<u8>>> {
        let entry = self.verify_id(id, user_id).await?;

        let take_res = entry.take();
        if matches!(take_res, Err(NotFound(_))) {
            self.cache.remove(id).await;
        }

        let Some(payload) = take_res? else {
            return Ok(None);
        };

        self.cache.remove(id).await;

        Ok(Some(payload))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod test {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn crockford_charset_adherence_and_length() {
        for _ in 0..10_000 {
            let id = Mailbox::generate_id();
            assert_eq!(id.len(), 8, "Mailbox ID must be exactly 8 characters");

            for b in id.bytes() {
                assert!(
                    BASE32_ALPHABET.contains(&b),
                    "Character '{b}' is not in Crockford Base32 alphabet"
                );
                // Crockford Base32 excludes i, l, o, u to prevent visual ambiguity
                assert_ne!(b, b'i');
                assert_ne!(b, b'l');
                assert_ne!(b, b'o');
                assert_ne!(b, b'u');
                // Crockford Base32 uses lowercase here
                assert!(!b.is_ascii_uppercase());
            }
        }
    }

    #[tokio::test]
    async fn id_generation_collision_safety_and_entropy() {
        let mailbox = Mailbox::new();
        let user_id = 42;
        let mut seen = HashSet::new();

        for _ in 0..1_000 {
            let id = mailbox.create(user_id).await;
            assert_eq!(id.len(), 8);
            assert!(seen.insert(id), "Duplicate mailbox ID generated");
        }
    }

    #[tokio::test]
    async fn complete_lifecycle_create_pending_fulfill_pickup() {
        let mailbox = Mailbox::new();
        let user_id = 100;
        let id = mailbox.create(user_id).await;

        // 1. Initial state: Pending -> pickup returns Ok(None)
        let poll = mailbox.pickup(&id, user_id).await;
        assert!(matches!(poll, Ok(None)));

        // 2. Fulfill with payload
        let payload = vec![0xca, 0xfe, 0xba, 0xbe];
        let fulfill_res = mailbox.fulfill(&id, user_id, payload.clone()).await;
        assert!(fulfill_res.is_ok());

        // 3. Pickup returns payload
        let pickup_res = mailbox.pickup(&id, user_id).await;
        assert_eq!(pickup_res.unwrap(), Some(payload));
    }

    #[tokio::test]
    async fn single_use_burn_after_reading() {
        let mailbox = Mailbox::new();
        let user_id = 100;
        let id = mailbox.create(user_id).await;

        let payload = vec![1, 2, 3, 4, 5];
        mailbox
            .fulfill(&id, user_id, payload.clone())
            .await
            .unwrap();

        // First pickup succeeds
        let res1 = mailbox.pickup(&id, user_id).await.unwrap();
        assert_eq!(res1, Some(payload));

        // Second pickup burns: returns NotFound
        let res2 = mailbox.pickup(&id, user_id).await;
        assert!(matches!(res2, Err(NotFound(_))));

        // Third pickup still returns NotFound
        let res3 = mailbox.pickup(&id, user_id).await;
        assert!(matches!(res3, Err(NotFound(_))));
    }

    #[tokio::test]
    async fn double_fulfill_rejection() {
        let mailbox = Mailbox::new();
        let user_id = 100;
        let id = mailbox.create(user_id).await;

        let payload1 = vec![1, 2, 3];
        let payload2 = vec![4, 5, 6];

        // First fulfill succeeds
        assert!(
            mailbox
                .fulfill(&id, user_id, payload1.clone())
                .await
                .is_ok()
        );

        // Second fulfill is rejected with BadRequest
        let second = mailbox.fulfill(&id, user_id, payload2).await;
        assert!(matches!(
            second,
            Err(BadRequest("Mailbox already fulfilled"))
        ));

        // Original payload remains intact on pickup
        let pickup = mailbox.pickup(&id, user_id).await.unwrap();
        assert_eq!(pickup, Some(payload1));
    }

    #[tokio::test]
    async fn cross_tenant_isolation_returns_not_found() {
        let mailbox = Mailbox::new();
        let user_a = 1;
        let user_b = 2;

        let id = mailbox.create(user_a).await;

        // User B attempts to fulfill User A's mailbox -> 404 NotFound (never 401/403)
        let fulfill_cross = mailbox.fulfill(&id, user_b, vec![9, 9, 9]).await;
        assert!(matches!(fulfill_cross, Err(NotFound(_))));

        // User B attempts to pickup User A's mailbox -> 404 NotFound
        let pickup_cross = mailbox.pickup(&id, user_b).await;
        assert!(matches!(pickup_cross, Err(NotFound(_))));

        // User B attempts to verify User A's mailbox -> 404 NotFound
        let verify_cross = mailbox.verify_id(&id, user_b).await;
        assert!(matches!(verify_cross, Err(NotFound(_))));

        // User A's mailbox is untampered and still pending
        assert!(matches!(mailbox.pickup(&id, user_a).await, Ok(None)));

        // User A fulfills normally
        assert!(mailbox.fulfill(&id, user_a, vec![7, 7, 7]).await.is_ok());

        // User B still cannot pickup fulfilled mailbox
        assert!(matches!(
            mailbox.pickup(&id, user_b).await,
            Err(NotFound(_))
        ));

        // User A picks up payload
        assert_eq!(
            mailbox.pickup(&id, user_a).await.unwrap(),
            Some(vec![7, 7, 7])
        );
    }

    #[tokio::test]
    async fn nonexistent_mailbox_returns_not_found() {
        let mailbox = Mailbox::new();
        let user_id = 1;
        let fake_id = "00000000";

        assert!(matches!(
            mailbox.verify_id(fake_id, user_id).await,
            Err(NotFound(_))
        ));
        assert!(matches!(
            mailbox.fulfill(fake_id, user_id, vec![1]).await,
            Err(NotFound(_))
        ));
        assert!(matches!(
            mailbox.pickup(fake_id, user_id).await,
            Err(NotFound(_))
        ));
    }

    #[tokio::test]
    async fn fulfill_after_pickup_consumed_returns_not_found() {
        let mailbox = Mailbox::new();
        let user_id = 1;
        let id = mailbox.create(user_id).await;

        mailbox.fulfill(&id, user_id, vec![1, 2, 3]).await.unwrap();
        mailbox.pickup(&id, user_id).await.unwrap();

        // Since pickup removes the entry from the cache, fulfilling now returns NotFound
        let res = mailbox.fulfill(&id, user_id, vec![4, 5, 6]).await;
        assert!(matches!(res, Err(NotFound(_))));
    }

    #[test]
    fn entry_take_transitions() {
        let entry = Entry {
            user_id: 1,
            status: Arc::new(Mutex::new(Pending)),
        };

        // When Pending, take returns Ok(None) and status stays Pending
        assert_eq!(entry.take().unwrap(), None);

        // Transition to Fulfilled
        *entry.status.lock().unwrap() = Fulfilled(vec![42]);

        // First take returns Some(payload) and sets status to Consumed
        assert_eq!(entry.take().unwrap(), Some(vec![42]));

        // Second take returns NotFound
        assert!(matches!(entry.take(), Err(NotFound(_))));
    }

    #[test]
    fn crockford_alphabet_full_coverage() {
        let mut seen_chars = HashSet::new();
        // Over 2,000 IDs (16,000 characters), every one of the 32 Crockford characters
        // must be observed at least once with overwhelming probability.
        for _ in 0..2_000 {
            let id = Mailbox::generate_id();
            for ch in id.chars() {
                seen_chars.insert(ch);
            }
        }

        assert_eq!(
            seen_chars.len(),
            32,
            "All 32 Crockford Base32 characters must be generated without bias"
        );
        for &expected_byte in BASE32_ALPHABET {
            assert!(
                seen_chars.contains(&char::from(expected_byte)),
                "Alphabet character '{}' was never generated",
                char::from(expected_byte)
            );
        }
    }

    #[tokio::test]
    async fn concurrent_pickup_single_winner() {
        let mailbox = Mailbox::new();
        let user_id = 42;
        let id = mailbox.create(user_id).await;
        let payload = vec![1, 2, 3, 4, 5, 6, 7, 8];
        mailbox
            .fulfill(&id, user_id, payload.clone())
            .await
            .unwrap();

        let mut tasks = Vec::new();
        for _ in 0..20 {
            let mb = mailbox.clone();
            let id_clone = id.clone();
            tasks.push(tokio::spawn(async move {
                mb.pickup(&id_clone, user_id).await
            }));
        }

        let mut success_count = 0;
        let mut not_found_count = 0;

        for task in tasks {
            let res = task.await.unwrap();
            assert!(
                matches!(res, Ok(Some(_)) | Err(NotFound(_))),
                "Outcome must be either Ok(Some) or Err(NotFound)"
            );
            if let Ok(Some(p)) = res {
                assert_eq!(p, payload);
                success_count += 1;
            } else if matches!(res, Err(NotFound(_))) {
                not_found_count += 1;
            }
        }

        assert_eq!(
            success_count, 1,
            "Exactly one concurrent pickup must succeed"
        );
        assert_eq!(
            not_found_count, 19,
            "All other concurrent pickups must receive NotFound"
        );
    }

    #[tokio::test]
    async fn concurrent_fulfill_single_winner() {
        let mailbox = Mailbox::new();
        let user_id = 42;
        let id = mailbox.create(user_id).await;

        let mut tasks = Vec::new();
        for i in 0u8..20u8 {
            let mb = mailbox.clone();
            let id_clone = id.clone();
            let payload = vec![i; 10];
            tasks.push(tokio::spawn(async move {
                mb.fulfill(&id_clone, user_id, payload).await
            }));
        }

        let mut success_count = 0;
        let mut conflict_count = 0;

        for task in tasks {
            let res = task.await.unwrap();
            assert!(
                res.is_ok()
                    || matches!(
                        res,
                        Err(BadRequest("Mailbox already fulfilled"))
                    ),
                "Outcome must be either Ok(()) or Err(BadRequest)"
            );
            if res.is_ok() {
                success_count += 1;
            } else if matches!(
                res,
                Err(BadRequest("Mailbox already fulfilled"))
            ) {
                conflict_count += 1;
            }
        }

        assert_eq!(
            success_count, 1,
            "Exactly one concurrent fulfill must succeed"
        );
        assert_eq!(
            conflict_count, 19,
            "All other concurrent fulfills must receive BadRequest"
        );

        // Verification: Pickup returns the payload from the winning fulfill
        let picked = mailbox.pickup(&id, user_id).await.unwrap();
        assert!(picked.is_some());
    }

    #[tokio::test]
    async fn fulfill_on_in_memory_consumed_entry_returns_not_found() {
        let mailbox = Mailbox::new();
        let user_id = 1;
        let id = mailbox.create(user_id).await;

        let entry = mailbox.verify_id(&id, user_id).await.unwrap();
        // Manually simulate status transitioning to Consumed while entry still in cache
        *entry.status.lock().unwrap() = Consumed;

        // Fulfill must return NotFound (not BadRequest)
        let res = mailbox.fulfill(&id, user_id, vec![1, 2, 3]).await;
        assert!(matches!(res, Err(NotFound(_))));
    }

    #[tokio::test]
    async fn cache_capacity_eviction_behavior() {
        let mailbox = Mailbox::new();
        let user_id = 1;

        // Create 1,050 mailboxes (exceeding max_capacity of 1,000)
        let mut ids = Vec::with_capacity(1_050);
        for _ in 0..1_050 {
            ids.push(mailbox.create(user_id).await);
        }

        assert_eq!(ids.len(), 1_050);

        // Let background maintenance run
        mailbox.cache.run_pending_tasks().await;

        // Cache entry count should be bounded by capacity
        assert!(mailbox.cache.entry_count() <= 1_000);
    }

    #[test]
    fn entry_take_poisoned_mutex_returns_internal_error() {
        let entry = Entry {
            user_id: 1,
            status: Arc::new(Mutex::new(Pending)),
        };

        let status_clone = Arc::clone(&entry.status);
        let _ = std::panic::catch_unwind(move || {
            let _guard = status_clone.lock().unwrap();
            panic!("deliberate panic to poison mutex");
        });

        let res = entry.take();
        assert!(matches!(
            res,
            Err(crate::api::Error::Internal(InternalErr::Message(
                "Mailbox mutex guard poisoned"
            )))
        ));
    }

    #[tokio::test]
    async fn mailbox_fulfill_and_pickup_poisoned_mutex() {
        let mailbox = Mailbox::new();
        let user_id = 1;
        let id = mailbox.create(user_id).await;

        let entry = mailbox.verify_id(&id, user_id).await.unwrap();
        let status_clone = Arc::clone(&entry.status);
        let _ = std::panic::catch_unwind(move || {
            let _guard = status_clone.lock().unwrap();
            panic!("deliberate panic to poison mutex");
        });

        // Fulfill returns Internal error
        let fulfill_res = mailbox.fulfill(&id, user_id, vec![1, 2, 3]).await;
        assert!(matches!(
            fulfill_res,
            Err(crate::api::Error::Internal(InternalErr::Message(
                "Mailbox mutex guard poisoned"
            )))
        ));

        // Pickup returns Internal error
        let pickup_res = mailbox.pickup(&id, user_id).await;
        assert!(matches!(
            pickup_res,
            Err(crate::api::Error::Internal(InternalErr::Message(
                "Mailbox mutex guard poisoned"
            )))
        ));
    }

    #[tokio::test]
    async fn concurrent_mailbox_create_unique_ids() {
        let mailbox = Mailbox::new();
        let user_id = 10;

        let mut tasks = Vec::new();
        for _ in 0..50 {
            let mb = mailbox.clone();
            tasks.push(tokio::spawn(async move { mb.create(user_id).await }));
        }

        let mut ids = HashSet::new();
        for task in tasks {
            let id = task.await.unwrap();
            assert_eq!(id.len(), 8);
            assert!(ids.insert(id), "Concurrent create produced duplicate ID");
        }
        assert_eq!(ids.len(), 50);
    }
}
