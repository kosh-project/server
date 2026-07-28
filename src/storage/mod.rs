pub mod error;
pub mod service;
use std::{error::Error as StdErr};

use bytes::Bytes;
use futures::Stream;
pub use service::Service;
pub mod transaction;

pub mod file;
pub use error::{Error, Result};

#[cfg(test)]
mod tests;


pub struct Payload<S, E> 
where 
    S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
    E: Into<Box<dyn StdErr + Send + Sync>>,{
    pub expected_size: u64,
    pub stream: S,
}

impl <S, E> Payload<S, E> 
where 
    S: Stream<Item = std::result::Result<Bytes, E>> + Unpin,
    E: Into<Box<dyn StdErr + Send + Sync>>,
    {
        pub fn new<T>(expected_size : T, stream : S) -> Self
        where T : Into<u64> {
            Self {
                expected_size : expected_size.into(),
                stream,
            } 
        }

}