use std::{fmt, path::Path};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Namespace {
    Files,
    Gallery,
}

impl Namespace {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Gallery => "gallery",
        }
    }
}

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl AsRef<Path> for Namespace {
    fn as_ref(&self) -> &Path {
        Path::new(self.as_str())
    }
}

impl TryFrom<&str> for Namespace {
    type Error = &'static str;
    fn try_from(string: &str) -> Result<Self, Self::Error> {
        match string {
            "files" => Ok(Self::Files),
            "gallery" => Ok(Self::Gallery),
            _ => Err("invalid namespace: expected 'files' or 'gallery'"),
        }
    }
}
