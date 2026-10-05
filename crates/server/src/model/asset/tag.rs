use serde_repr::{Deserialize_repr, Serialize_repr};

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
    GalleryMeta = 0,
    GalleryItem = 1,
    DriveMeta = 2,
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
