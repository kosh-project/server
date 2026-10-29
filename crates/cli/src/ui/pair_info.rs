use kosh_core::{config::Config, tls};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, HorizontalAlignment, Layout, Rect},
    style::{Color, Stylize},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget},
};

use crate::core::pairing;

#[derive(Default)]
pub struct PairInfo {
    pub is_visible: bool,
    pub is_online: bool,
    pub data: pairing::Data,
}

impl PairInfo {
    pub async fn try_from(config: &Config) -> anyhow::Result<Self> {
        let cert_path = config.tls_identity_path();
        let cert = tls::Identity::load(
            cert_path.join("cert.pem"),
            cert_path.join("key.pem"),
        )
        .await?;

        let bytes = cert.fingerprint_raw()?;

        let data = pairing::Data::new(config, &bytes)?;

        Ok(Self {
            is_visible: false,
            is_online: false,
            data,
        })
    }

    #[inline]
    pub fn help_line() -> Line<'static> {
        Line::from(vec![" <p> - ".blue().bold(), "hide this menu".into()])
    }
}

impl Widget for &PairInfo {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        // render_heading(area, buf);

        Clear.render(area, buf);

        let inner_area = self.render_outer_frame(area, buf);

        let qr_height = self.data.qr_lines.len() as u16;
        let qr_width =
            self.data.qr_lines.first().map_or(0, |l| l.width()) as u16;

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(qr_height.max(8)), // QR code height
                Constraint::Length(5),                // Words height
                Constraint::Length(3),                // Fingerprint height
            ])
            .split(inner_area);

        let top_columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(32), Constraint::Length(qr_width)])
            .split(rows[0]);

        self.render_addresses(top_columns[0], buf);
        self.render_qr(top_columns[1], buf);
        self.render_words(rows[1], buf);
        self.render_fingerprint(rows[2], buf);
    }
}

impl PairInfo {
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
        Paragraph::new(self.data.server_address.clone())
            .render(vert_chunks[1], buf);
    }

    #[inline]
    fn render_qr(&self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.data.qr_lines.clone()).render(area, buf);
    }

    #[inline]
    fn render_words(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .title(" Verification Words ".reset().bold())
            .border_type(BorderType::Rounded)
            .border_style(Color::Green);

        let inner_area = block.inner(area);
        block.render(area, buf);

        Paragraph::new(self.data.words.clone())
            .fg(Color::Gray)
            .render(inner_area, buf);
    }

    #[inline]
    fn render_fingerprint(&self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .title(" TLS Fingerprint ".reset().bold())
            .border_type(BorderType::Rounded)
            .border_style(Color::Green);

        let inner_area = block.inner(area);
        block.render(area, buf);

        Line::from(vec![
            "sha256/".bold().gray(),
            self.data.fingerprint.clone().reset(),
        ])
        .alignment(HorizontalAlignment::Center)
        .render(inner_area, buf);
    }
}

fn _render_heading(area: Rect, buf: &mut Buffer) {
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
        y: area.y.saturating_sub(6),
        ..area
    };

    heading.render(rect, buf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use kosh_core::config::{Config, Mdns, Passkey};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use tmpdir::TmpDir;

    #[test]
    fn test_help_line() {
        let line = PairInfo::help_line();
        let rendered = line.to_string();
        assert!(rendered.contains("<p>"));
        assert!(rendered.contains("hide this menu"));
    }

    #[tokio::test]
    async fn test_try_from_succeeds_with_tls_identity() -> anyhow::Result<()> {
        let temp_dir = TmpDir::new("pair-info-test")
            .await
            .context("create temp dir")?;
        let vault_path = temp_dir.to_path_buf();

        let _ = tls::Identity::load_or_create(&vault_path, &["kosh-ui.local"])
            .await
            .context("generate test tls identity")?;

        let config = Config {
            vault_path,
            port: 23342,
            host: "0.0.0.0".to_string(),
            enable_tls: true,
            socket_path: "/tmp/kosh.sock".to_string(),
            passkey: Passkey::default(),
            mdns: Mdns {
                enable: true,
                hostname: "kosh-ui".to_string(),
                ..Default::default()
            },
        };

        let pair_info = PairInfo::try_from(&config)
            .await
            .context("PairInfo::try_from should succeed")?;

        assert!(!pair_info.is_visible);
        assert!(!pair_info.is_online);
        assert!(pair_info.data.tls_enabled);
        assert_eq!(pair_info.data.words.len(), 3);
        assert!(!pair_info.data.qr_lines.is_empty());

        let address_text: Vec<String> = pair_info
            .data
            .server_address
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        assert!(address_text.iter().any(|l| l.contains("kosh-ui.local")));

        // Test widget rendering
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        Widget::render(&pair_info, area, &mut buf);

        Ok(())
    }

    #[tokio::test]
    async fn test_try_from_fails_when_cert_missing() -> anyhow::Result<()> {
        let temp_dir = TmpDir::new("pair-info-missing")
            .await
            .context("create temp dir")?;

        let config = Config {
            vault_path: temp_dir.to_path_buf(),
            port: 23342,
            host: "0.0.0.0".to_string(),
            enable_tls: true,
            socket_path: "/tmp/kosh.sock".to_string(),
            passkey: Passkey::default(),
            mdns: Mdns::default(),
        };

        let result = PairInfo::try_from(&config).await;
        assert!(
            result.is_err(),
            "PairInfo::try_from must fail when cert.pem is absent"
        );

        Ok(())
    }
}
