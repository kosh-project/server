use serde_repr::{Deserialize_repr, Serialize_repr};

/// Identifies the category and intended storage partition of an asset.
///
/// Tags separate client-side media and files into discrete domains (Gallery vs Drive)
/// as well as separating lightweight encrypted metadata descriptors from heavyweight
/// encrypted binary blobs.
#[derive(
    sqlx::Type,
    Copy,
    Clone,
    Serialize_repr,
    Deserialize_repr,
    PartialEq,
    Eq,
    Debug,
)]
#[repr(i32)]
pub enum Tag {
    /// Encrypted metadata or thumbnail record for a gallery photo or video.
    GalleryMeta = 0,
    /// Encrypted full-resolution binary payload for a gallery photo or video.
    GalleryItem = 1,
    /// Encrypted metadata or directory hierarchy descriptor for drive files.
    DriveMeta = 2,
    /// Encrypted binary payload for a generic drive file.
    DriveItem = 3,
}

impl From<Tag> for i32 {
    #[allow(clippy::as_conversions)]
    fn from(tag: Tag) -> Self {
        tag as Self
    }
}

impl TryFrom<&str> for Tag {
    type Error = ();

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        use Tag::{DriveItem, DriveMeta, GalleryItem, GalleryMeta};

        Ok(match value {
            "0" => GalleryMeta,
            "1" => GalleryItem,
            "2" => DriveMeta,
            "3" => DriveItem,
            _ => return Err(()),
        })
    }
}
