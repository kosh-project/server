use anyhow::Result;
use base64::Engine;
use bip39::{Language, Mnemonic};
use kosh_core::config::Config;
use local_ip_address::list_afinet_netifas;
use qrcodegen::{QrCode, QrCodeEcc};
use ratatui::{style::Stylize, text::Line};

/// Pure data generated once on startup for the Device Pairing UI.
/// This prevents recalculating hashes and formatting strings in the hot render loop.
#[derive(Default, Clone)]
pub struct Data {
    pub tls_enabled: bool,
    pub fingerprint: String,
    pub words: Vec<Line<'static>>,
    pub qr_lines: Vec<Line<'static>>,
    pub server_address: Vec<Line<'static>>,
}

impl Data {
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

        assert!(address_text.iter().any(|line| line.contains("localhost:23342")));

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
        assert!(address_text.iter().any(|line| line.contains("localhost:23342")));

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
        let words1: Vec<String> = data1.words.iter().map(std::string::ToString::to_string).collect();
        let words2: Vec<String> = data2.words.iter().map(std::string::ToString::to_string).collect();
        assert_eq!(words1, words2);

        let qr1: Vec<String> = data1.qr_lines.iter().map(std::string::ToString::to_string).collect();
        let qr2: Vec<String> = data2.qr_lines.iter().map(std::string::ToString::to_string).collect();
        assert_eq!(qr1, qr2);

        Ok(())
    }
}

