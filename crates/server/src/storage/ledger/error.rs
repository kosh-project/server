use tokio::io;

use crate::storage::ledger::action::Action;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Comitter died before an active sender")]
    ComitterDead,

    #[error("Failed to open or write to ledger segment")]
    IoError(#[from] io::Error),
}
