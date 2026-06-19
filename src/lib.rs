pub mod api;
pub mod app;
pub mod log;
pub mod storage;

pub mod error;
use std::{io::Read, slice::Iter};

use bytes::Bytes;
pub use error::{Error, Result};

pub fn encode(bytes_iter: Iter<u8>) -> String {
    bytes_iter.map(|x| format!("{x:02x}")).collect()
}
