use std::{collections::HashSet, net::IpAddr, time::Duration};

use kosh_core::config::Config;
use local_ip_address::list_afinet_netifas;
use mdns_sd::{ServiceDaemon, ServiceInfo};

pub struct Broadcaster {
    daemon: ServiceDaemon,
    fullname: String,
}

impl Broadcaster {
    /// Starts the mDNS daemon and registers the service info based on configuration.
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

    pub(crate) fn resolve_addresses(configured_interfaces: &[String]) -> Vec<IpAddr> {
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

    pub fn shutdown(self) {
        if let Ok(receiver) = self.daemon.unregister(&self.fullname) {
            let _ = receiver.recv_timeout(Duration::from_millis(500));
        }
        let _ = self.daemon.shutdown();
    }
}
