//! Background mDNS service daemon lifecycle, address resolution, and announcement engine.

use std::{collections::HashSet, net::IpAddr, time::Duration};

use kosh_core::config::Config;
use local_ip_address::list_afinet_netifas;
use mdns_sd::{ServiceDaemon, ServiceInfo};

/// An active Multicast DNS service broadcaster handle.
///
/// Wraps an underlying [`ServiceDaemon`] thread and the registered service's fully-qualified
/// domain name (`fullname`). While this handle remains alive, the daemon automatically responds
/// to multicast queries on UDP port 5353 and broadcasts unsolicited announcement packets
/// across the configured network interfaces.
///
/// Call [`Broadcaster::shutdown`] during server shutdown to send goodbye packets and terminate
/// the daemon thread cleanly.
pub struct Broadcaster {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Broadcaster {
    /// Starts the mDNS daemon and registers the service info based on configuration.
    ///
    /// Determines service parameters from [`Config`]:
    /// - **Service type**: `_https._tcp.local.` if TLS is enabled, else `_http._tcp.local.`.
    /// - **Instance name & hostname**: Uses [`kosh_core::config::Mdns::hostname`] formatted as `<hostname>.local.`.
    /// - **IP addresses**: Discovered via `Self::resolve_addresses` according to configured interface filters.
    /// - **Port**: Advertises the active server HTTP/HTTPS port.
    /// - **TXT properties**: Key-value pairs configured in [`kosh_core::config::Mdns::txt`].
    ///
    /// # Arguments
    ///
    /// * `config` - Server configuration specifying port, TLS mode, and mDNS settings.
    ///
    /// # Errors
    ///
    /// Returns [`mdns_sd::Error`] if service info construction, daemon startup, or registration fails.
    pub fn start(config: &Config) -> Result<Self, mdns_sd::Error> {
        let service_type = if config.enable_tls {
            "_https._tcp.local."
        } else {
            "_http._tcp.local."
        };

        let instance_name = &config.mdns.hostname;

        let hostname = format!("{}.local.", config.mdns.hostname);

        let addresses = Self::resolve_addresses(&config.mdns.interfaces);

        let mut properties: Vec<(&str, &str)> =
            Vec::with_capacity(config.mdns.txt.properties.len());

        for prop in &config.mdns.txt.properties {
            properties.push((prop.key.as_str(), prop.value.as_str()));
        }

        let service_info = ServiceInfo::new(
            service_type,
            instance_name,
            &hostname,
            &addresses[..],
            config.port,
            &properties[..],
        )?;

        let fullname = service_info.get_fullname().to_string();

        let daemon = ServiceDaemon::new()?;
        daemon.register(service_info)?;

        Ok(Self { daemon, fullname })
    }

    /// Resolves active, non-loopback IP addresses across host network interfaces.
    ///
    /// If `configured_interfaces` is non-empty, only interfaces whose system names match
    /// an entry in the list are included (e.g. `eth0`, `wlan0`). If `configured_interfaces`
    /// is empty, all active non-loopback IPv4 and IPv6 addresses are included.
    ///
    /// # Arguments
    ///
    /// * `configured_interfaces` - Slice of interface name filters specified in configuration.
    ///
    /// # Returns
    ///
    /// A vector of unique [`IpAddr`] instances discovered on the matching interfaces.
    pub(crate) fn resolve_addresses(
        configured_interfaces: &[String],
    ) -> Vec<IpAddr> {
        let mut unique_ips: HashSet<IpAddr> =
            HashSet::with_capacity(configured_interfaces.len());

        if let Ok(interfaces) = list_afinet_netifas() {
            let filter_active = !configured_interfaces.is_empty();

            for (iface_name, ip) in interfaces {
                if ip.is_loopback() {
                    continue;
                }

                if filter_active
                    && !configured_interfaces
                        .iter()
                        .any(|cfg| cfg == &iface_name)
                {
                    continue;
                }

                unique_ips.insert(ip);
            }
        }

        unique_ips.into_iter().collect()
    }

    /// Gracefully unregisters the service and stops the background mDNS daemon.
    ///
    /// Sends a DNS "goodbye" packet with TTL = 0 to notify local network peers that the service
    /// is going offline immediately, allowing clients to remove the service from their discovery
    /// caches without waiting for cache timeouts. Waits up to 500ms for unregistration confirmation
    /// before shutting down the daemon.
    pub fn shutdown(self) {
        if let Ok(receiver) = self.daemon.unregister(&self.fullname) {
            let _ = receiver.recv_timeout(Duration::from_millis(500));
        }
        let _ = self.daemon.shutdown();
    }
}
