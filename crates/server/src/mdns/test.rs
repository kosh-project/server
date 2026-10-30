#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]

use super::*;
use kosh_core::config::{Config, Mdns, MdnsProperty, MdnsTxt, Passkey};
use std::path::PathBuf;

fn make_test_config(enable_mdns: bool, enable_tls: bool) -> Config {
    Config {
        vault_path: PathBuf::from("./vault"),
        port: 45678,
        host: "0.0.0.0".to_string(),
        enable_tls,
        socket_path: "/tmp/kosh.sock".to_string(),
        passkey: Passkey::default(),
        mdns: Mdns {
            enable: enable_mdns,
            hostname: "kosh-unit-test".to_string(),
            interfaces: Vec::new(),
            txt: MdnsTxt {
                properties: vec![MdnsProperty {
                    span: knuffel::span::Span(0, 0),
                    key: "test_key".to_string(),
                    value: "test_val".to_string(),
                }],
                ..Default::default()
            },
            ..Default::default()
        },
    }
}

#[test]
fn mdns_init_returns_none_when_disabled() {
    let config = make_test_config(false, false);
    let broadcaster = init(&config);
    assert!(broadcaster.is_none());
}

#[test]
fn mdns_init_and_shutdown_when_enabled_http() {
    let config = make_test_config(true, false);
    let broadcaster = init(&config);
    assert!(broadcaster.is_some());
    if let Some(b) = broadcaster {
        b.shutdown();
    }
}

#[test]
fn mdns_init_and_shutdown_when_enabled_https() {
    let mut config = make_test_config(true, true);
    config.mdns.hostname = "kosh-https-test".to_string();
    let broadcaster = init(&config);
    assert!(broadcaster.is_some());
    if let Some(b) = broadcaster {
        b.shutdown();
    }
}

#[test]
fn resolve_addresses_filters_loopback() {
    let addrs = Broadcaster::resolve_addresses(&[]);
    for addr in addrs {
        assert!(
            !addr.is_loopback(),
            "Resolved address {addr} must not be loopback"
        );
    }
}

#[test]
fn resolve_addresses_with_nonexistent_interface_returns_empty() {
    let addrs =
        Broadcaster::resolve_addresses(&["nonexistent_iface_9999".to_string()]);
    assert!(
        addrs.is_empty(),
        "Filtering for non-existent interface must return empty"
    );
}

#[test]
fn broadcaster_start_and_shutdown_with_custom_txt() {
    let mut config = make_test_config(true, false);
    config.mdns.hostname = "kosh-txt-test".to_string();
    config.mdns.txt.properties.push(MdnsProperty {
        span: knuffel::span::Span(0, 0),
        key: "role".to_string(),
        value: "storage-node".to_string(),
    });

    let broadcaster = Broadcaster::start(&config);
    assert!(broadcaster.is_ok());
    if let Ok(b) = broadcaster {
        b.shutdown();
    }
}

#[test]
fn broadcaster_start_fails_on_empty_hostname() {
    let mut config = make_test_config(true, false);
    config.mdns.hostname = String::new();
    let broadcaster = Broadcaster::start(&config);
    assert!(
        broadcaster.is_err(),
        "Broadcaster::start must fail with empty hostname"
    );
}

#[test]
fn mdns_init_returns_none_when_broadcaster_fails() {
    let mut config = make_test_config(true, false);
    config.mdns.hostname = String::new();
    let broadcaster = init(&config);
    assert!(
        broadcaster.is_none(),
        "mdns::init must return None when Broadcaster fails"
    );
}

#[test]
fn resolve_addresses_with_loopback_only_interface_returns_empty() {
    let addrs = Broadcaster::resolve_addresses(&["lo".to_string()]);
    assert!(
        addrs.is_empty(),
        "Filtering exclusively for loopback interface must yield empty addresses"
    );
}

#[test]
fn broadcaster_start_with_filtered_nonexistent_interface() {
    let mut config = make_test_config(true, false);
    config.mdns.hostname = "kosh-nonexist-iface".to_string();
    config.mdns.interfaces = vec!["nonexistent_iface_999".to_string()];

    let broadcaster = Broadcaster::start(&config);
    if let Ok(b) = broadcaster {
        b.shutdown();
    }
}
