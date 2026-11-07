/// Lifecycle status of an ephemeral key exchange mailbox.
///
/// Models the atomic state machine for device pairing:
/// - Flow A: `Ready` -> `Fulfilled` -> `Consumed`
/// - Flow B: `Waiting` -> `Ready` -> `Fulfilled` -> `Consumed`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Mailbox offered by authenticated sender, waiting for receiver to join (Flow B).
    Waiting {
        /// Account ID of the authenticated user who initiated the pairing mailbox.
        user_id: UserId,
    },
    /// Ephemeral public key deposited; ready for sender to fulfill with ciphertext.
    Ready {
        /// Optional account ID bound if initiated via Flow B (`None` if opened via Flow A).
        user_id: Option<UserId>,
        /// Ephemeral public key (32 bytes) deposited by receiver.
        pub_temp: Vec<u8>,
        /// BLAKE3 keyed hash of the secret polling token issued to the receiver.
        token_hash: [u8; 32],
    },
    /// Sender has fulfilled the mailbox by depositing the encrypted pairing payload.
    Fulfilled {
        /// Encrypted ciphertext payload (contains registration ticket and envelope keys).
        ciphertext: Vec<u8>,
        /// BLAKE3 keyed hash of the receiver's polling token for authorization.
        token_hash: [u8; 32],
    },
    /// Mailbox payload consumed and burned from RAM.
    Consumed,
}

use crate::app::state::UserId;
