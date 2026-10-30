#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]

use anyhow::Context;
use kosh_core::config::{Config, Mdns, MdnsProperty, MdnsTxt, Passkey};
use server::mdns::{self, Broadcaster};
use std::time::Duration;
use tmpdir::TmpDir;

#[tokio::test]
async fn test_mdns_broadcast_and_shutdown_lifecycle() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-mdns-test")
        .await
        .context("create temp dir")?;

    let config = Config {
        vault_path: temp_dir.to_path_buf(),
        port: 49152,
        host: "0.0.0.0".to_string(),
        enable_tls: false,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: Passkey::default(),
        mdns: Mdns {
            enable: true,
            hostname: "kosh-e2e-node".to_string(),
            interfaces: Vec::new(),
            txt: MdnsTxt {
                properties: vec![
                    MdnsProperty {
                        span: knuffel::span::Span(0, 0),
                        key: "version".to_string(),
                        value: "0.1.36".to_string(),
                    },
                    MdnsProperty {
                        span: knuffel::span::Span(0, 0),
                        key: "tls".to_string(),
                        value: "disabled".to_string(),
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        },
    };

    // Start broadcaster
    let broadcaster = Broadcaster::start(&config)
        .context("failed to start mDNS broadcaster")?;

    // Verify service can be browsed and resolved
    let daemon = mdns_sd::ServiceDaemon::new().context("client daemon")?;
    let receiver = daemon.browse("_http._tcp.local.").context("browse")?;

    let mut discovered = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) =
            receiver.recv_timeout(Duration::from_millis(200))
            && info.get_fullname().contains("kosh-e2e-node")
        {
            assert_eq!(info.get_port(), 49152);
            assert_eq!(info.get_property_val_str("version"), Some("0.1.36"));
            assert_eq!(info.get_property_val_str("tls"), Some("disabled"));
            discovered = true;
            break;
        }
    }

    assert!(
        discovered,
        "Advertised mDNS service was not discovered within timeout"
    );

    // Shutdown client and broadcaster cleanly
    let _ = daemon.shutdown();
    broadcaster.shutdown();

    Ok(())
}

#[tokio::test]
async fn test_mdns_disabled_lifecycle() -> anyhow::Result<()> {
    let temp_dir = TmpDir::new("kosh-mdns-disabled-test")
        .await
        .context("create temp dir")?;

    let config = Config {
        vault_path: temp_dir.to_path_buf(),
        port: 49153,
        host: "0.0.0.0".to_string(),
        enable_tls: true,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: Passkey::default(),
        mdns: Mdns {
            enable: false,
            hostname: "kosh-disabled".to_string(),
            ..Default::default()
        },
    };

    let broadcaster = mdns::init(&config);
    assert!(broadcaster.is_none());

    Ok(())
}
