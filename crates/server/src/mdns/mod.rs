//! Multicast DNS (mDNS) and DNS-SD zero-configuration service discovery broadcaster.
//!
//! This module manages the background advertising of the Kosh server on local area networks
//! using Multicast DNS ([RFC 6762](https://datatracker.ietf.org/doc/html/rfc6762)) and
//! DNS-Based Service Discovery ([RFC 6763](https://datatracker.ietf.org/doc/html/rfc6763)).
//!
//! # Overview
//!
//! When enabled in the server configuration ([`kosh_core::config::Config::mdns`]):
//! - A [`Broadcaster`](crate::mdns::Broadcaster) is started via [`init`](crate::mdns::init), spawning a background daemon that announces
//!   the service on multicast port 5353.
//! - Service type is automatically determined based on [`kosh_core::config::Config::enable_tls`]:
//!   `_https._tcp.local.` when TLS is enabled, or `_http._tcp.local.` when disabled.
//! - Hostname is published as `<hostname>.local.`.
//! - IP addresses are resolved across specified network interfaces (or all non-loopback interfaces if unspecified).
//! - Custom key-value attributes from [`kosh_core::config::Config::mdns`] are included as DNS-SD TXT records.
//!
//! # Fault Tolerance
//!
//! Failure to start mDNS broadcasting (e.g. if multicast socket binding fails or a firewall blocks UDP 5353)
//! emits a warning log but does not crash the server boot process. The server will continue serving HTTP/HTTPS
//! requests normally.

mod broadcast;

pub use broadcast::Broadcaster;
use kosh_core::{config::Config, logger::Module};

use crate::{info, warn};

/// Initializes and starts the mDNS discovery broadcaster if enabled in configuration.
///
/// Checks [`config.mdns.enable`](kosh_core::config::Mdns::enable):
/// - If `false`, mDNS broadcasting is skipped and returns `None`.
/// - If `true`, attempts to launch the [`Broadcaster`] background daemon. On success,
///   logs the active `<hostname>.local` broadcast and port, returning `Some(Broadcaster)`.
///   On error, logs a warning and returns `None` so the server can proceed to boot without mDNS.
///
/// # Arguments
///
/// * `config` - Server configuration containing port, TLS mode, and mDNS settings.
///
/// # Returns
///
/// `Some(Broadcaster)` if successfully started, or `None` if disabled or initialization failed.
pub fn init(config: &Config) -> Option<Broadcaster> {
    if !config.mdns.enable {
        return None;
    }

    match Broadcaster::start(config) {
        Ok(broadcaster) => {
            info!(
                Module::Server,
                "mDNS discovery active: broadcasting `{}.local` on port {}",
                config.mdns.hostname,
                config.port
            );
            Some(broadcaster)
        }
        Err(e) => {
            warn!(Module::Server, "Failed to initialize mDNS broadcaster: {e}");
            None
        }
    }
}

#[cfg(test)]
mod test;
