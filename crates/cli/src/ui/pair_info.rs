use kosh_core::{config::Config, tls};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, HorizontalAlignment, Layout, Rect},
    style::{Color, Stylize},
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Widget},
};

use crate::core::pairing::PairingData;

#[derive(Default)]
pub struct PairInfo {
    pub is_visible: bool,
    pub is_online: bool,
    pub data: PairingData,
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

        let data = PairingData::new(config.enable_tls, config.port, &bytes)?;

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
