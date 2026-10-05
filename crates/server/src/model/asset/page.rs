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
    /// Unix timestamp (seconds) of the boundary asset row from the previous page.
    pub before_time: i64,
    /// 32-character hexadecimal UUID of the boundary asset row used to break timestamp collisions.
    pub before_id: String,
}

impl AssetCursor {
    /// Decodes the hex string into raw 16-byte blob for SQLite binding.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Internal`] if the cursor identifier is not valid hexadecimal.
    pub fn decode_id(&self) -> Result<Vec<u8>> {
        hex::decode(&self.before_id).map_err(|_| {
            Error::Internal(internal::Error::Message("Invalid cursor hex"))
        })
    }
}

/// Metadata representing an asset ownership row returned in a paginated list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataRow {
    /// The BLAKE3 hex-encoded hash identifying the underlying storage blob.
    pub hash: String,
    /// The size of the asset blob in bytes.
    pub size_bytes: i64,
    /// The Unix timestamp (seconds) when the asset was created or last modified.
    pub last_modified: i64,
    /// The category classification of this asset.
    pub tag: Tag,
}

/// A paginated result set of asset metadata rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    /// The list of asset metadata rows fetched for the current page.
    pub assets: Vec<MetadataRow>,
    /// Keyset cursor to retrieve the next page, or `None` if this was the last page.
    pub next_cursor: Option<AssetCursor>,
}
