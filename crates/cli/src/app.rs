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

use crate::{
    entry::{self, Entry, List},
    help::Help,
};
pub struct App {
    entries: entry::List,
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
            entry_list = Some(List::init_or_default(&bytes));
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

    const fn filter_open(&self) -> bool {
        self.entries.filter.is_open
    }

    fn handle_key(&mut self, key_event: &KeyEvent) {
        if key_event.is_press() | key_event.is_repeat() {
            match key_event.code {
                KeyCode::Esc => self.handle_esc(),
                KeyCode::Char(_)
                | KeyCode::BackTab
                | KeyCode::Backspace
                | KeyCode::Tab
                    if self.filter_open() =>
                {
                    self.entries.handle_filter(key_event)
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.should_quit = true
                }
                KeyCode::Char('/') => self.entries.filter.is_open = true,
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.show_help = !self.show_help
                }
                _ => self.entries.handle_key(key_event),
            }
        }
    }

    fn handle_esc(&mut self) {
        if self.show_help {
            self.show_help = false;
        } else if self.filter_open() {
            self.entries.filter.is_open = false;
        }
    }

    pub(crate) fn append(&mut self, bytes: &[u8]) {
        if let Ok((entry, _)) =
            decode_from_slice::<logger::Entry, _>(bytes, config::standard())
        {
            self.entries.add_log(entry);
        }
    }

    fn help_line(&self) -> Line<'_> {
        if self.show_help {
            Line::from(vec![" <h> -".bold().blue(), " Back to Main ".into()])
        } else if self.filter_open() {
            Line::from(self.entries.filter.help_line())
        } else {
            Line::from(vec![
                " <h> -".bold().blue(),
                " Help ".into(),
                " <j/k> -".bold().blue(),
                " Scroll".into(),
            ])
        }
    }

    #[inline]
    const fn constraints(&self) -> &[Constraint] {
        if self.filter_open() {
            &[
                Constraint::Fill(1),
                Constraint::Length(3),
                Constraint::Length(1),
            ]
        } else {
            &[Constraint::Fill(1), Constraint::Length(1)]
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self {
            entries: List::default(),
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
            .constraints(self.constraints())
            .split(area);

        let block =
            Block::default().border_style(Style::new().fg(Color::Green));

        self.entries.render(chunks[0], buf);

        let footer_text = Paragraph::new(self.help_line())
            .block(block)
            .alignment(HorizontalAlignment::Center);

        footer_text.render(
            if self.entries.filter.is_open {
                chunks[2]
            } else {
                chunks[1]
            },
            buf,
        );

        if self.filter_open() {
            self.entries.filter.render(chunks[1], buf);
        }

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
