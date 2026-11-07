//! In-memory ephemeral ticket cache for device pairing registration.
//!
//! Authenticated devices mint a single-use registration ticket (`POST /api/v1/devices/link-ticket`)
//! that expires after 5 minutes (300 seconds). A newly paired device presents this ticket
//! to register its passkey credential without requiring an existing session token.

use std::time::Duration;

use moka::future::Cache;

use crate::app::state::UserId;

/// In-memory cache holding ephemeral registration tickets for device pairing.
///
/// Backed by [`moka::future::Cache`] with a 300-second Time-To-Live (TTL) and
/// a maximum capacity of 1,000 entries.
#[derive(Clone)]
pub struct Ticket {
    pub cache: Cache<String, UserId>,
}

impl Default for Ticket {
    fn default() -> Self {
        Self::new()
    }
}

impl Ticket {
    /// Time-to-live for a registration ticket in seconds (5 minutes).
    pub const TTL_SECS: u64 = 300;

    /// Maximum number of active tickets stored in memory.
    pub const MAX_CAPACITY: u64 = 1_000;

    /// Constructs a new ticket cache with default TTL and capacity limits.
    #[must_use]
    pub fn new() -> Self {
        let cache = Cache::builder()
            .time_to_live(Duration::from_secs(Self::TTL_SECS))
            .max_capacity(Self::MAX_CAPACITY)
            .build();

        Self { cache }
    }

    /// Mints a new cryptographically secure random 32-byte hex ticket bound to `user_id`.
    ///
    /// Stores `(ticket, user_id)` in the cache and returns the 64-character hex string.
    pub async fn mint(&self, user_id: UserId) -> String {
        let bytes: [u8; 32] = rand::random();
        let token = hex::encode(bytes);
        self.cache.insert(token.clone(), user_id).await;
        token
    }

    /// Validates ticket ownership without consuming it.
    ///
    /// Returns `Some(UserId)` if the ticket exists and has not expired, or `None` otherwise.
    pub async fn peek(&self, ticket: &str) -> Option<UserId> {
        self.cache.get(ticket.trim()).await
    }

    /// Atomically consumes and invalidates the ticket, returning the associated `UserId`.
    ///
    /// Returns `Some(UserId)` on the first consumption, or `None` if the ticket does not exist,
    /// has expired, or was already consumed (single-use semantics).
    pub async fn consume(&self, ticket: &str) -> Option<UserId> {
        self.cache.remove(ticket.trim()).await
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test]
    async fn ticket_mint_and_peek() {
        let ticket_cache = Ticket::new();
        let user_id: UserId = 42;

        let token = ticket_cache.mint(user_id).await;
        assert_eq!(token.len(), 64);
        assert!(hex::decode(&token).is_ok());

        let peeked = ticket_cache.peek(&token).await;
        assert_eq!(peeked, Some(user_id));

        // Peek does not consume
        let peeked_again = ticket_cache.peek(&token).await;
        assert_eq!(peeked_again, Some(user_id));
    }

    #[tokio::test]
    async fn ticket_consume_single_use() {
        let ticket_cache = Ticket::new();
        let user_id: UserId = 99;

        let token = ticket_cache.mint(user_id).await;
        let consumed = ticket_cache.consume(&token).await;
        assert_eq!(consumed, Some(user_id));

        // Second consume returns None
        let consumed_again = ticket_cache.consume(&token).await;
        assert_eq!(consumed_again, None);

        // Peek after consume returns None
        let peeked = ticket_cache.peek(&token).await;
        assert_eq!(peeked, None);
    }

    #[tokio::test]
    async fn ticket_nonexistent() {
        let ticket_cache = Ticket::new();
        assert_eq!(ticket_cache.peek("nonexistent").await, None);
        assert_eq!(ticket_cache.consume("nonexistent").await, None);
    }

    #[tokio::test]
    async fn ticket_whitespace_handling() {
        let ticket_cache = Ticket::new();
        let user_id: UserId = 77;

        let token = ticket_cache.mint(user_id).await;
        let padded = format!("  {token}  \n");

        assert_eq!(ticket_cache.peek(&padded).await, Some(user_id));
        assert_eq!(ticket_cache.consume(&padded).await, Some(user_id));
        assert_eq!(ticket_cache.peek(&token).await, None);
    }
}
