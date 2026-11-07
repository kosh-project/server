/// Lifecycle status of an ephemeral key exchange mailbox.
///
/// Models the one-way atomic state transitions of an ephemeral mailbox entry:
/// `Pending` -> `Fulfilled(Vec<u8>)` -> `Consumed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Mailbox created by receiver, awaiting sender fulfillment.
    Waiting { user_id: UserId },
    /// Mailbox fulfilled by sender with raw payload bytes.
    Ready {
        user_id: Option<UserId>,
        pub_temp: Vec<u8>,
        token_hash: [u8; 32],
    },
    Fulfilled {
        ciphertext: Vec<u8>,
        token_hash: [u8; 32],
    },
    /// Mailbox payload consumed and burned by receiver.
    Consumed,
}

use crate::app::state::UserId;
