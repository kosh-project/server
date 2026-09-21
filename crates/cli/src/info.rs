use std::fmt::Alignment;

use anyhow::Result;
use base64::prelude::*;
use bip39::{Language, Mnemonic};
use kosh_core::{config::Config, tls};
use local_ip_address::list_afinet_netifas;
use qrcodegen::{QrCode, QrCodeEcc};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, HorizontalAlignment, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, ToSpan},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget},
};

#[derive(Default)]
pub struct PairInfo {
    pub tls_enabled: bool,
    pub is_visible: bool,
    pub fingerprint: Line<'static>,
    pub words: Paragraph<'static>,
    pub qr: Paragraph<'static>,
    pub server_address: Vec<Line<'static>>,
    pub is_online: bool,
}

impl PairInfo {
    pub async fn try_from(config: &Config) -> Result<Self> {
        let cert_path = config.tls_identity_path();
        let cert = tls::Identity::load(
            cert_path.join("cert.pem"),
            cert_path.join("key.pem"),
        )
        .await?;

        let bytes = cert.fingerprint_raw()?;

        let words = words_from(&bytes)?;

        let fingerprint = Line::from(vec![
            "sha256/".bold().gray(),
            BASE64_STANDARD.encode(&bytes).reset(),
        ])
        .alignment(HorizontalAlignment::Center);

        let qr = qr_from(&bytes)?;

        let pair_info = Self {
            is_visible: false,
            tls_enabled: config.enable_tls,
            words,
            fingerprint,
            qr: Paragraph::new(qr),
            server_address: resolve_display_addresses(config.port),
            is_online: false,
        };

        Ok(pair_info)
    }
}

fn words_from(bytes: &[u8]) -> Result<Paragraph<'static>> {
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
                list[i + 0],
                list[i + 1],
                list[i + 2],
                list[i + 3],
            ))
            .alignment(HorizontalAlignment::Center),
        );
    }

    Ok(Paragraph::new(lines))
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

impl Widget for &PairInfo {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        render_heading(area, buf);

        Clear.render(area, buf);

        let inner_area = self.render_outer_frame(area, buf);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(25),
                Constraint::Length(5),
                Constraint::Length(3),
            ])
            .split(inner_area);

        let top_columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(32), Constraint::Length(25)])
            .split(rows[0]);

        self.render_addresses(top_columns[0], buf);
        self.render_qr(top_columns[1], buf);
        self.render_words(rows[1], buf);
        self.render_fingerprint(rows[2], buf);
    }
}

impl PairInfo {
    #[inline]
    pub fn help_line() -> Line<'static> {
        Line::from(vec![" <p> - ".blue().bold(), "hide this menu".into()])
    }

    #[inline]
    fn render_outer_frame(&self, area: Rect, buf: &mut Buffer) -> Rect {
        let block = Block::default()
            .title("<  Device Pairing  >")
            .title_alignment(HorizontalAlignment::Center)
            .borders(Borders::ALL)
            .padding(Padding::new(2, 2, 1, 0))
            .border_type(BorderType::Rounded);

        let inner = block.inner(area);

        block.render(area, buf);

        inner
    }

    #[inline]
    fn render_addresses(&self, area: Rect, buf: &mut Buffer) {
        let vert_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(3)])
            .split(area);

        let status = if self.is_online {
            "ONLINE ◉".green()
        } else {
            "OFFLINE ◇".reset()
        };

        Line::from(vec!["Status : ".bold(), status])
            .render(vert_chunks[0], buf);
        Paragraph::new(self.server_address.clone()).render(vert_chunks[1], buf);
    }

    #[inline]
    fn render_qr(&self, area: Rect, buf: &mut Buffer) {
        self.qr.as_ref().render(area, buf);
    }

    #[inline]
    fn render_words(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .title(" Verification Words ".reset().bold())
            .border_type(BorderType::Rounded)
            .border_style(Color::Green);

        let inner_area = block.inner(area);

        block.render(area, buf);
        self.words.as_ref().render(inner_area, buf);
    }

    #[inline]
    fn render_fingerprint(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .title(" TLS Fingerprint ".reset().bold())
            .border_type(BorderType::Rounded)
            .border_style(Color::Green);

        (&self.fingerprint).render(block.inner(area), buf);

        block.render(area, buf);
    }
}

fn render_heading(area: Rect, buf: &mut Buffer) {
    let heading = Paragraph::new(
        r#"      ___           _              ___      _      _              .
    /   \_____   _(_) ___ ___    / _ \__ _(_)_ __(_)_ __   __ _ .
   / /\ / _ \ \ / / |/ __/ _ \  / /_)/ _` | | '__| | '_ \ / _` |
  / /_//  __/\ V /| | (_|  __/ / ___/ (_| | | |  | | | | | (_| |
 /___,' \___| \_/ |_|\___\___| \/    \__,_|_|_|  |_|_| |_|\__, |
                                                          |___/ "#,
    )
    .centered();

    let rect = Rect {
        y: area.y - 6,
        ..area
    };

    heading.render(rect, buf);
}
