pub mod api;
pub mod app;
pub mod log;
pub mod model;
pub mod storage;

pub mod error;
use std::{io::Read, slice::Iter};

use bytes::Bytes;
pub use error::{Error, Result};
