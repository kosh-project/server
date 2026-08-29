mod action;
mod committer;
mod error;
mod handle;
mod segment;

pub use error::Error;
pub type Result<T> = core::result::Result<T, Error>;

pub(crate) use action::AppendReciept;
