use base64::Engine;
use anyhow::Result;
use bip39::{Language, Mnemonic};
use local_ip_address::list_afinet_netifas;
use qrcodegen::{QrCode, QrCodeEcc};
use ratatui::{
    style::Stylize,
    text::Line,
};

/// Pure data generated once on startup for the Device Pairing UI.
/// This prevents recalculating hashes and formatting strings in the hot render loop.
#[derive(Default, Clone)]
pub struct PairingData {
    pub tls_enabled: bool,
    pub fingerprint: String,
    pub words: Vec<Line<'static>>,
    pub qr_lines: Vec<Line<'static>>,
    pub server_address: Vec<Line<'static>>,
}

impl PairingData {
    pub fn new(tls_enabled: bool, port: u16, cert_fingerprint: &[u8]) -> Result<Self> {
        let words = words_from(cert_fingerprint)?;
        let qr_lines = qr_from(cert_fingerprint)?;
        let server_address = resolve_display_addresses(port);
        let fingerprint = base64::prelude::BASE64_STANDARD.encode(cert_fingerprint);

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
    let entropy: [u8; 16] = bytes[0..16].try_into()?;
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

fn resolve_display_addresses(port: u16) -> Vec<Line<'static>> {
    let mut addresses = Vec::with_capacity(5);

    addresses.push(Line::from("---------- SERVER ADDRESSES ----------".bold()));

    addresses.push(Line::from(vec![
        "Local : ".bold(),
        "localhost:".into(),
        port.to_string().into(),
    ]));

    if let Ok(interfaces) = list_afinet_netifas() {
        for (name, ip) in interfaces {
            if ip.is_ipv4() {
                if ip.is_loopback() {
                    addresses.push(Line::from(vec![
                        "Local : ".bold(),
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
