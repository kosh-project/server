#![allow(clippy::as_conversions)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::panic_in_result_fn)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;

#[test]
fn level_repr_u8_stability() {
    assert_eq!(Level::Info as u8, 0);
    assert_eq!(Level::Warning as u8, 1);
    assert_eq!(Level::Error as u8, 2);
    assert_eq!(Level::Fatal as u8, 3);
    assert_eq!(Level::Shutdown as u8, 4);
}

#[test]
fn module_repr_u8_stability() {
    assert_eq!(Module::Server as u8, 0);
    assert_eq!(Module::Database as u8, 1);
    assert_eq!(Module::Storage as u8, 2);
    assert_eq!(Module::Asset as u8, 3);
    assert_eq!(Module::Api as u8, 5);
    assert_eq!(Module::Ledger as u8, 6);
    assert_eq!(Module::Logger as u8, 7);
}

#[test]
fn entry_bincode_roundtrip() {
    let entry = Entry {
        module: Module::Storage,
        level: Level::Warning,
        timestamp_ms: 1_725_000_123_456,
        message: "Storage subsystem warning: disk near capacity".to_string(),
    };

    let encoded =
        bincode_next::encode_to_vec(&entry, bincode_next::config::standard())
            .expect("bincode encoding should succeed");
    assert!(!encoded.is_empty());

    let (decoded, read_len): (Entry, usize) = bincode_next::decode_from_slice(
        &encoded,
        bincode_next::config::standard(),
    )
    .expect("bincode decoding should succeed");

    assert_eq!(read_len, encoded.len());
    assert_eq!(decoded.module, Module::Storage);
    assert_eq!(decoded.level, Level::Warning);
    assert_eq!(decoded.timestamp_ms, 1_725_000_123_456);
    assert_eq!(
        decoded.message,
        "Storage subsystem warning: disk near capacity"
    );
}

#[test]
fn telemetry_log_variant_roundtrip() {
    let entry = Entry {
        module: Module::Api,
        level: Level::Info,
        timestamp_ms: 1_700_000_000_000,
        message: "HTTP request processed successfully".to_string(),
    };
    let telemetry = Telemetry::Log(entry);

    let encoded = bincode_next::encode_to_vec(
        &telemetry,
        bincode_next::config::standard(),
    )
    .expect("bincode encoding should succeed");

    let (decoded, len): (Telemetry, usize) = bincode_next::decode_from_slice(
        &encoded,
        bincode_next::config::standard(),
    )
    .expect("bincode decoding should succeed");

    assert_eq!(len, encoded.len());
    match decoded {
        Telemetry::Log(e) => {
            assert_eq!(e.module, Module::Api);
            assert_eq!(e.level, Level::Info);
            assert_eq!(e.message, "HTTP request processed successfully");
        }
        Telemetry::Heartbeat => {
            panic!("Expected Telemetry::Log, found Heartbeat")
        }
    }
}

#[test]
fn telemetry_heartbeat_variant_roundtrip() {
    let telemetry = Telemetry::Heartbeat;

    let encoded = bincode_next::encode_to_vec(
        &telemetry,
        bincode_next::config::standard(),
    )
    .expect("bincode encoding should succeed");

    let (decoded, len): (Telemetry, usize) = bincode_next::decode_from_slice(
        &encoded,
        bincode_next::config::standard(),
    )
    .expect("bincode decoding should succeed");

    assert_eq!(len, encoded.len());
    match decoded {
        Telemetry::Heartbeat => {}
        Telemetry::Log(_) => panic!("Expected Telemetry::Heartbeat, found Log"),
    }
}
