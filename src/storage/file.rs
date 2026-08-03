use std::{
    fs::Metadata as StdMeta, os::unix::fs::MetadataExt, time::UNIX_EPOCH,
};

use blake3::Hash;

use crate::storage::{Result};


pub struct Metadata {
    pub hash: Hash,
    pub last_modified: i64,
    pub size: i64,
}

impl Metadata {
    /// Attempts to construct a new `Metadata` instance from standard library metadata.
    ///
    /// # Errors
    /// Returns an error if the system time is earlier than `UNIX_EPOCH` or if integer conversions fail.
    pub fn try_new(metadata: &StdMeta, hash: Hash) -> Result<Self> {
        Ok(Self {
            hash,
            last_modified: metadata
                .modified()?
                .duration_since(UNIX_EPOCH)?
                .as_secs()
                .try_into()?,
            size: metadata.size()
                .try_into()?
        })
    }
}
