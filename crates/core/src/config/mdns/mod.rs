//! Multicast DNS (mDNS) discovery configuration and validation.

mod diagnostic;
#[allow(clippy::module_inception)]
mod mdns;
mod txt;

pub use diagnostic::Diagnostic;
pub use mdns::Mdns;
pub use txt::{Property, Txt};
