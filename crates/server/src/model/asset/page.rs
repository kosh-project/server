use serde::{Deserialize, Serialize};

use crate::{
    error::internal,
    model::{Error, asset::Tag, error::Result},
};

/// Keyset cursor for pagination.
///
/// Contains the boundary values of the last item in the previous page.
/// Used to seek directly to the next page in O(log N) time using the composite index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetCursor {
    pub before_time: i64,
    pub before_id: String,
}

impl AssetCursor {
    /// Decodes the hex string into raw 16-byte blob for SQLite binding.
    pub fn decode_id(&self) -> Result<Vec<u8>> {
        hex::decode(&self.before_id).map_err(|_| {
            Error::Internal(internal::Error::Message("Invalid cursor hex"))
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRow {
    pub hash: String,
    pub size_bytes: i64,
    pub last_modified: i64,
    pub tag: Tag,
}

/// A paginated result set of asset metadata rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub assets: Vec<MetadataRow>,
    pub next_cursor: Option<AssetCursor>,
}
