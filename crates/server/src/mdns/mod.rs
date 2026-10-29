mod broadcast;

pub use broadcast::Broadcaster;
use kosh_core::{config::Config, logger::Module};

use crate::{info, warn};

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

