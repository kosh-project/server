use std::fs;

use bincode_next::{
    config::{self, Configuration},
    decode_from_slice,
};
use chrono::Utc;
use crossterm::event::{Event, KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, HorizontalAlignment, Layout, Rect},
    style::Stylize,
    text::Line,
    widgets::{Paragraph, Widget},
};
use tokio::runtime::Handle;
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
        if key_event.is_press() {
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

        let line = Line::from(vec![
            " <h> -".bold().blue(),
            " Help ".into(),
            " <↑/↓> -".bold().blue(),
            " Scroll".into(),
        ]);

        if self.show_help {
            Help.render(area, buf);
        }

        self.entries.render(chunks[0], buf);

        let footer_text =
            Paragraph::new(line).alignment(HorizontalAlignment::Center);

        footer_text.render(chunks[1], buf);
    }
}
