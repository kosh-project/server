//! Cryptographic device pairing data generator for Trust-On-First-Use (TOFU) verification.
//!
//! When connecting a client device (e.g. the Kosh Android app) to a self-hosted server for the
//! first time, the client must verify the authenticity of the server's self-signed TLS certificate
//! to prevent Machine-In-The-Middle (MITM) attacks.
//!
//! This module precomputes all terminal UI rendering components once during startup:
//!
//! - **Base64 Fingerprint** — Standard SHA-256 certificate digest string.
//! - **BIP-39 Verification Words** — 12 human-readable mnemonic words derived from the first 16 bytes
//!   (128 bits of entropy) of the certificate fingerprint. This allows users to easily confirm that the
//!   fingerprint on their mobile phone matches the server terminal with zero typing.
//! - **Unicode Half-Block QR Code** — High-density terminal QR code generated using Unicode block
//!   elements (`▀`, `▄`, `█`, ` `) so mobile cameras can scan connection details directly off the screen.
//! - **Server Discovery Addresses** — Formatted list of LAN IP endpoints and active mDNS `.local` hostnames.

use anyhow::Result;
use base64::Engine;
use bip39::{Language, Mnemonic};
use kosh_core::config::Config;
use local_ip_address::list_afinet_netifas;
use qrcodegen::{QrCode, QrCodeEcc};
use ratatui::{style::Stylize, text::Line};

/// Precomputed device pairing data cached for terminal UI rendering.
///
/// Pre-generating and caching these lines avoids expensive cryptographic hashing,
/// BIP-39 dictionary lookups, QR code matrix evaluations, and interface resolution
/// during the 60 FPS terminal redraw loop.
#[derive(Default, Clone)]
pub struct Data {
    /// Whether TLS encryption is active on the server.
    pub tls_enabled: bool,
    /// Base64-encoded representation of the 32-byte SHA-256 TLS certificate fingerprint.
    pub fingerprint: String,
    /// Three lines of formatted BIP-39 mnemonic verification words (4 words per line).
    pub words: Vec<Line<'static>>,
    /// Multi-line Unicode half-block QR code representation of the raw certificate fingerprint.
    pub qr_lines: Vec<Line<'static>>,
    /// Formatted server connection endpoints (mDNS, localhost, and active network interfaces).
    pub server_address: Vec<Line<'static>>,
}

impl Data {
    /// Derives device pairing data from the active server configuration and TLS certificate fingerprint.
    ///
    /// # Arguments
    ///
    /// * `config` - Active server configuration specifying port, TLS mode, and mDNS settings.
    /// * `cert_fingerprint` - Raw 32-byte SHA-256 certificate fingerprint. Must be at least 16 bytes long.
    ///
    /// # Errors
    ///
    /// - Returns an error if `cert_fingerprint` contains fewer than 16 bytes.
    /// - Returns an error if BIP-39 mnemonic generation or QR code encoding fails.
    pub fn new(config: &Config, cert_fingerprint: &[u8]) -> Result<Self> {
        let words = words_from(cert_fingerprint)?;
        let qr_lines = qr_from(cert_fingerprint)?;
        let server_address = resolve_display_addresses(config);
        let fingerprint =
            base64::prelude::BASE64_STANDARD.encode(cert_fingerprint);
        let tls_enabled = config.enable_tls;

        Ok(Self {
            tls_enabled,
            fingerprint,
            words,
            qr_lines,
            server_address,
        })
    }
}

/// Derives a 12-word BIP-39 English mnemonic phrase from the first 16 bytes of entropy.
///
/// 128 bits of certificate fingerprint entropy uniquely map to 12 words from the standard
/// BIP-39 English dictionary, centered and split into 3 lines of 4 words for terminal display.
///
/// # Arguments
///
/// * `bytes` - Raw byte slice containing at least 16 bytes of certificate fingerprint entropy.
///
/// # Returns
///
/// A vector of 3 formatted [`Line`] instances containing centered mnemonic words.
///
/// # Errors
///
/// Returns an error if `bytes` has fewer than 16 bytes or if BIP-39 derivation fails.
fn words_from(bytes: &[u8]) -> Result<Vec<Line<'static>>> {
    let slice = bytes.get(..16).ok_or_else(|| {
        anyhow::anyhow!(
            "fingerprint slice too short: expected at least 16 bytes, got {}",
            bytes.len()
        )
    })?;
    let entropy: [u8; 16] = slice.try_into()?;
    let mnemonic = Mnemonic::from_entropy_in(Language::English, &entropy)?;

    let mut list = [""; 12];
    for (i, word) in mnemonic.words().enumerate() {
        list[i] = word;
    }

    let mut lines = Vec::with_capacity(3);
    for i in (0..list.len()).step_by(4) {
        lines.push(
            Line::from(format!(
                "{:<8}    {:<8}    {:<8}    {:<8}",
                list[i],
                list[i + 1],
                list[i + 2],
                list[i + 3],
            ))
            .alignment(ratatui::layout::HorizontalAlignment::Center),
        );
    }

    Ok(lines)
}

/// Encodes raw bytes into a compact terminal-compatible QR code using Unicode half-block characters.
///
/// Terminal character cells typically have an approximate 1:2 aspect ratio (taller than they are wide).
/// By combining two vertical QR modules into one character cell using Unicode upper half block (`▀`),
/// lower half block (`▄`), full block (`█`), or space (`\u{00A0}`), the resulting QR code renders with
/// square proportions that can be reliably scanned by mobile cameras.
///
/// # Arguments
///
/// * `bytes` - Raw byte payload to encode into the QR matrix.
///
/// # Returns
///
/// A vector of [`Line`] strings ready for terminal paragraph rendering.
///
/// # Errors
///
/// Returns an error if QR code matrix generation fails.
fn qr_from(bytes: &[u8]) -> Result<Vec<Line<'static>>> {
    let binary_qr = QrCode::encode_binary(bytes, QrCodeEcc::Low)?;
    let size = binary_qr.size();
    let mut qr_lines = Vec::with_capacity(size as usize);

    for y in (0..size).step_by(2) {
        let mut row_string = String::with_capacity(size as usize * 3 + 4);

        for x in 0..size {
            let top = binary_qr.get_module(x, y);
            let bottom = if y + 1 < size {
                binary_qr.get_module(x, y + 1)
            } else {
                false
            };

            let c = match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => '\u{00A0}',
            };
            row_string.push(c);
        }
        qr_lines.push(Line::from(row_string));
    }
    Ok(qr_lines)
}

/// Formats the list of server network addresses for display in the pairing overlay.
///
/// Discovers and styles:
/// - Active mDNS `<hostname>.local:<port>` address (if enabled).
/// - Loopback addresses (`localhost:<port>`, `127.0.0.1:<port>`).
/// - Network interface IPv4 addresses with associated device names (e.g. `eth0`, `wlan0`).
///
/// # Arguments
///
/// * `config` - Active server configuration containing port, TLS mode, and mDNS settings.
///
/// # Returns
///
/// Formatted lines ready for rendering in the network endpoints block.
fn resolve_display_addresses(config: &Config) -> Vec<Line<'static>> {
    let mut addresses = Vec::with_capacity(5);
    let port = config.port;

    addresses.push(Line::from("---------- SERVER ADDRESSES ----------".bold()));

    if config.mdns.enable {
        addresses.push(Line::from(vec![
            "mDNS    : ".bold(),
            format!("{}.local", config.mdns.hostname).bold(),
            ':'.reset(),
            port.to_string().into(),
        ]));
    }

    addresses.push(Line::from(vec![
        "Local   : ".bold(),
        "localhost:".into(),
        port.to_string().into(),
    ]));

    if let Ok(interfaces) = list_afinet_netifas() {
        for (name, ip) in interfaces {
            if ip.is_ipv4() {
                if ip.is_loopback() {
                    addresses.push(Line::from(vec![
                        "Local   : ".bold(),
                        ip.to_string().into(),
                        ':'.reset(),
                        port.to_string().into(),
                    ]));
                } else {
                    addresses.push(Line::from(vec![
                        "Network : ".bold(),
                        ip.to_string().into(),
                        ':'.reset(),
                        port.to_string().into(),
                        " (".green(),
                        name.into(),
                        ')'.green(),
                    ]));
                }
            }
        }
    }

    addresses
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample_config(mdns_enable: bool, hostname: &str, port: u16) -> Config {
        Config {
            vault_path: PathBuf::from("./vault"),
            port,
            host: "0.0.0.0".to_string(),
            enable_tls: true,
            socket_path: "/tmp/kosh.sock".to_string(),
            passkey: kosh_core::config::Passkey::default(),
            mdns: kosh_core::config::Mdns {
                enable: mdns_enable,
                hostname: hostname.to_string(),
                ..Default::default()
            },
        }
    }

    #[test]
    fn pairing_data_with_mdns_enabled_includes_mdns_line() -> Result<()> {
        let config = sample_config(true, "kosh-nas", 23342);
        let fingerprint = [0xabu8; 32];

        let data = Data::new(&config, &fingerprint)?;

        assert!(data.tls_enabled);
        assert_eq!(
            data.fingerprint,
            base64::prelude::BASE64_STANDARD.encode(fingerprint)
        );
        assert_eq!(data.words.len(), 3);
        assert!(!data.qr_lines.is_empty());

        let address_text: Vec<String> = data
            .server_address
            .iter()
            .map(std::string::ToString::to_string)
            .collect();

        let mdns_entry = address_text.iter().find(|line| line.contains("mDNS"));
        assert!(
            mdns_entry.is_some(),
            "expected mDNS entry in server addresses: {address_text:?}"
        );
        let mdns_line = mdns_entry.unwrap();
        assert!(mdns_line.contains("kosh-nas.local"));
        assert!(mdns_line.contains("23342"));

        assert!(
            address_text
                .iter()
                .any(|line| line.contains("localhost:23342"))
        );

        Ok(())
    }

    #[test]
    fn pairing_data_with_mdns_disabled_omits_mdns_line() -> Result<()> {
        let config = sample_config(false, "kosh-nas", 23342);
        let fingerprint = [0x12u8; 32];

        let data = Data::new(&config, &fingerprint)?;

        assert!(data.tls_enabled);
        let address_text: Vec<String> = data
            .server_address
            .iter()
            .map(std::string::ToString::to_string)
            .collect();

        assert!(
            !address_text.iter().any(|line| line.contains("mDNS")),
            "mDNS should not be displayed when disabled: {address_text:?}"
        );
        assert!(
            address_text
                .iter()
                .any(|line| line.contains("localhost:23342"))
        );

        Ok(())
    }

    #[test]
    fn pairing_data_short_fingerprint_returns_err() {
        let config = sample_config(true, "kosh", 23342);
        let short_fp = [0x55u8; 8];

        let result = Data::new(&config, &short_fp);
        assert!(
            result.is_err(),
            "fingerprint shorter than 16 bytes must return error"
        );
    }

    #[test]
    fn pairing_data_deterministic_output() -> Result<()> {
        let config = sample_config(true, "kosh", 23342);
        let fingerprint = [0x7fu8; 32];

        let data1 = Data::new(&config, &fingerprint)?;
        let data2 = Data::new(&config, &fingerprint)?;

        assert_eq!(data1.fingerprint, data2.fingerprint);
        let words1: Vec<String> = data1
            .words
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let words2: Vec<String> = data2
            .words
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(words1, words2);

        let qr1: Vec<String> = data1
            .qr_lines
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let qr2: Vec<String> = data2
            .qr_lines
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert_eq!(qr1, qr2);

        Ok(())
    }
}
