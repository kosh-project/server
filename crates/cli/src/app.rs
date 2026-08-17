use std::fs;

use bincode_next::{
    config::{self},
    decode_from_slice,
};
use chrono::Utc;
use crossterm::{
    event::{Event, KeyCode, KeyEvent},
    terminal,
};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, HorizontalAlignment, Layout, Rect},
    style::{Color, Style, Stylize},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};
use webdav_server::logger::{self, format_date_time};

use crate::{entry::Entry, help::Help, list::EntryList};
pub struct App {
    entries: EntryList,
    show_help: bool,
    pub should_quit: bool,
}

impl App {
    pub fn try_init() -> Option<Self> {
        let log_path = logger::path()?;
        let timestamp_millis = Utc::now().timestamp_millis();

        let mut entry_list = None;

        if let Ok(bytes) =
            fs::read(log_path.join(format_date_time(timestamp_millis)))
        {
            entry_list = Some(EntryList::init_or_default(&bytes));
        }

        let mut app = Self::default();
        app.entries = entry_list.unwrap_or_default();

        Some(app)
    }

    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.entries.handle_mouse(mouse),
            _ => {}
        }
    }

    fn handle_key(&mut self, key_event: &KeyEvent) {
        if key_event.is_press() | key_event.is_repeat() {
            match key_event.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.should_quit = true
                }
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.show_help = !self.show_help
                }
                _ => self.entries.handle_key(key_event),
            }
        }
    }

    pub(crate) fn append(&mut self, bytes: &[u8]) {
        if let Ok((entry, _)) =
            decode_from_slice::<logger::Entry, _>(bytes, config::standard())
        {
            self.entries.add_log(Entry::from(entry));
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            entries: EntryList::default(),
            show_help: false,
            should_quit: false,
        }
    }
}

impl Widget for &mut App {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Length(1)])
            .split(area);

        let block =
            Block::default().border_style(Style::new().fg(Color::Green));

        let line = Line::from(vec![
            " <h> -".bold().blue(),
            " Help ".into(),
            " <j/k> -".bold().blue(),
            " Scroll".into(),
        ]);

        self.entries.render(chunks[0], buf);

        let footer_text = Paragraph::new(line)
            .block(block)
            .alignment(HorizontalAlignment::Center);

        footer_text.render(chunks[1], buf);
        if self.show_help {
            let (width, height) = terminal::size().unwrap();
            let area = Rect {
                x: width.saturating_sub(50) / 2,
                y: height.saturating_sub(8) / 2,
                width: 50,
                height: 8,
            };
            Help.render(area, buf);
        }
    }
}
