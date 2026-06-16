use crate::storage;

pub enum Error {
    CommitFailed { id: u64 },
}

pub type Result<T> = Result<T, storage::Error>;
