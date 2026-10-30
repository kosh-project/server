//! Multicast DNS (mDNS) and DNS-SD zero-configuration discovery configuration and validation.
//!
//! This module defines the configuration structures, validation rules, and rich terminal
//! diagnostics for advertising the Kosh server on local area networks using Multicast DNS
//! ([RFC 6762](https://datatracker.ietf.org/doc/html/rfc6762)) and DNS-Based Service Discovery
//! ([RFC 6763](https://datatracker.ietf.org/doc/html/rfc6763)).
//!
//! # Architecture & Purpose
//!
//! In a self-hosted personal cloud, users frequently access the server from phones, laptops, and
//! tablets connected to home Wi-Fi or LAN networks without dedicated local DNS infrastructure.
//! Rather than forcing users to memorize dynamic DHCP IP addresses or configure static leases,
//! Kosh advertises its services under the standard `.local` link-local domain (e.g. `kosh.local`).
//!
//! When mDNS is enabled:
//! - The server advertises its HTTP/HTTPS service endpoint and active port across LAN interfaces.
//! - The advertised `<hostname>.local` domain and detected LAN IP addresses are automatically
//!   included as Subject Alternative Names (SANs) in self-signed TLS certificates.
//! - Client applications (such as the Kosh mobile app or CLI) can discover and connect to the
//!   server automatically without manual IP configuration.
//!
//! # KDL Configuration Syntax
//!
//! The `mdns` block is configured within the root KDL configuration file:
//!
//! ```kdl
//! mdns {
//!     enable true
//!     hostname "kosh-vault"
//!     interfaces "eth0" "wlan0"
//!     txt {
//!         property "version" "0.1.0"
//!         property "vendor" "kosh"
//!     }
//! }
//! ```
//!
//! # Validation Pipeline & Diagnostics
//!
//! Configuration validation occurs immediately after KDL decoding in [`crate::config::Config::load_or_init`].
//! Source spans are tracked via [`knuffel::span::Span`], allowing validation errors to emit rich
//! [`miette::Diagnostic`] reports with source code highlights, precise terminal squiggles, and
//! actionable fix recommendations.
//!
//! Checked rules include:
//! - **Hostname rules** ([RFC 1034](https://datatracker.ietf.org/doc/html/rfc1034), [RFC 1123](https://datatracker.ietf.org/doc/html/rfc1123)):
//!   Must be a single label between 1 and 63 ASCII alphanumeric characters or hyphens, without dots
//!   or leading/trailing hyphens.
//! - **Interface names**: Must be non-empty, at most 15 bytes, and contain valid interface characters.
//! - **TXT attributes** ([RFC 6763](https://datatracker.ietf.org/doc/html/rfc6763)):
//!   Keys must be printable ASCII without `=` (up to 32 bytes), values up to 128 bytes, and total TXT
//!   payload constrained to 400 bytes to avoid UDP packet fragmentation across Wi-Fi networks.

mod diagnostic;
#[allow(clippy::module_inception)]
mod mdns;
mod txt;

pub use diagnostic::Diagnostic;
pub use mdns::Mdns;
pub use txt::{Property, Txt};
