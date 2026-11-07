use std::sync::{Arc, Mutex, MutexGuard};

use crate::{
    app::{
        mailbox::{
            Error::{
                self, AlreadyFulfilled, AlreadyJoined, NotFound, NotReady,
                Unauthorized,
            },
            Result, Token,
            status::Status::{self, Consumed, Fulfilled, Ready, Waiting},
        },
        state::UserId,
    },
    error::internal,
};

/// Thread-safe cached mailbox entry holding user ownership and atomic lifecycle status.
#[derive(Clone)]
pub struct Entry {
    status: Arc<Mutex<Status>>,
}

#[allow(clippy::significant_drop_tightening)]
impl Entry {
    #[must_use]
    pub fn new_waiting(user_id: UserId) -> Self {
        Self {
            status: Arc::new(Mutex::new(Waiting { user_id })),
        }
    }

    #[must_use]
    pub fn new_ready(pub_temp: Vec<u8>, token_hash: [u8; 32]) -> Self {
        Self {
            status: Arc::new(Mutex::new(Ready {
                user_id: None,
                pub_temp,
                token_hash,
            })),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Status>> {
        self.status
            .lock()
            .map_err(|_| Error::Internal(internal::Error::MutexPoisoned))
    }

    /// Transitions a waiting mailbox to ready state upon receiver join.
    ///
    /// # Errors
    ///
    /// - Returns [`AlreadyJoined`] if another receiver has already joined.
    /// - Returns [`AlreadyFulfilled`] if the mailbox has already been fulfilled.
    /// - Returns [`NotFound`] if the mailbox has already been consumed.
    /// - Returns [`Internal`](Error::Internal) if the lock is poisoned.
    pub fn join(&self, pub_temp: Vec<u8>) -> Result<String> {
        let mut status = self.lock()?;
        match *status {
            Waiting { user_id } => {
                let token = Token::new();
                *status = Ready {
                    user_id: Some(user_id),
                    pub_temp,
                    token_hash: token.hash,
                };
                Ok(token.hex)
            }
            Ready { .. } => Err(AlreadyJoined),
            Fulfilled { .. } => Err(AlreadyFulfilled),
            Consumed => Err(NotFound),
        }
    }

    /// Checks the current pairing status and returns ephemeral public key if ready.
    ///
    /// # Errors
    ///
    /// - Returns [`NotFound`] if the caller is not the mailbox owner or entry is consumed.
    /// - Returns [`AlreadyFulfilled`] if the mailbox is already fulfilled.
    /// - Returns [`Internal`](Error::Internal) if the lock is poisoned.
    pub fn status(&self, caller_id: UserId) -> Result<Option<Vec<u8>>> {
        let status = self.lock()?;
        match &*status {
            Waiting { user_id } => {
                if *user_id != caller_id {
                    return Err(NotFound);
                }
                Ok(None)
            }
            Ready {
                user_id, pub_temp, ..
            } => {
                if user_id.is_some_and(|owner| owner != caller_id) {
                    return Err(NotFound);
                }
                Ok(Some(pub_temp.clone()))
            }
            Fulfilled { .. } => Err(AlreadyFulfilled),
            Consumed => Err(NotFound),
        }
    }

    /// Deposits the encrypted pairing ciphertext into a ready mailbox.
    ///
    /// # Errors
    ///
    /// - Returns [`NotReady`] if the mailbox is still waiting for a join.
    /// - Returns [`NotFound`] if the caller is not the owner or entry is consumed.
    /// - Returns [`AlreadyFulfilled`] if the mailbox was already fulfilled.
    /// - Returns [`Internal`](Error::Internal) if the lock is poisoned.
    pub fn fulfill(
        &self,
        caller_id: UserId,
        ciphertext: Vec<u8>,
    ) -> Result<()> {
        let mut status = self.lock()?;
        match *status {
            Waiting { user_id } => {
                if user_id != caller_id {
                    return Err(NotFound);
                }
                Err(NotReady)
            }
            Ready {
                user_id,
                token_hash,
                ..
            } => {
                if user_id.is_some_and(|owner| owner != caller_id) {
                    return Err(NotFound);
                }
                *status = Fulfilled {
                    ciphertext,
                    token_hash,
                };
                Ok(())
            }
            Fulfilled { .. } => Err(AlreadyFulfilled),
            Consumed => Err(NotFound),
        }
    }

    /// Polls or retrieves deposited payload using the receiver's polling token.
    ///
    /// # Errors
    ///
    /// - Returns [`Unauthorized`] if the provided token does not match.
    /// - Returns [`NotReady`] if the mailbox is still in waiting state.
    /// - Returns [`NotFound`] if the mailbox has already been consumed.
    /// - Returns [`Internal`](Error::Internal) if the lock is poisoned.
    pub fn pickup(&self, raw_token: &str) -> Result<Option<Vec<u8>>> {
        let token_hash = Token::hash_raw(raw_token);
        let mut status = self.lock()?;
        match &mut *status {
            Ready {
                token_hash: expected_hash,
                ..
            } => {
                if *expected_hash != token_hash {
                    return Err(Unauthorized);
                }
                Ok(None)
            }
            Fulfilled {
                ciphertext,
                token_hash: expected_hash,
            } => {
                if *expected_hash != token_hash {
                    return Err(Unauthorized);
                }
                let payload = std::mem::take(ciphertext);
                *status = Consumed;
                Ok(Some(payload))
            }
            Waiting { .. } => Err(NotReady),
            Consumed => Err(NotFound),
        }
    }
}
