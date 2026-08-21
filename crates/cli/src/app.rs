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
    entry::{self, List},
    help::Help,
};

#[derive(Default)]
pub struct App {
    entries: entry::List,
    show_help: bool,
    pub should_quit: bool,
}

impl App {
    pub fn try_init() -> Option<Self> {
        let log_path = logger::path()?;
        let timestamp_millis = Utc::now().timestamp_millis();

        let entry_list =
            fs::read(log_path.join(format_date_time(timestamp_millis)))
                .map_or(None, |bytes| Some(List::init_or_default(&bytes)));

        let app = Self {
            entries: entry_list.unwrap_or_default(),
            ..Default::default()
        };

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
                    self.entries.handle_filter(key_event);
                }
                KeyCode::Char('q' | 'Q') => {
                    self.should_quit = true;
                }
                KeyCode::Char('/') => {
                    self.entries.filter.is_open = true;
                }
                KeyCode::Char('h' | 'H') => {
                    self.show_help = !self.show_help;
                }
                _ => {
                    self.entries.handle_key(key_event);
                }
            }
        }
    }

    const fn handle_esc(&mut self) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    #[test]
    fn test_app_input_routing() {
        let mut app = App {
            entries: crate::entry::List::default(),
            show_help: false,
            should_quit: false,
        };

        assert!(!app.should_quit);
        assert!(!app.show_help);
        assert!(!app.entries.filter.is_open);

        app.handle_key(&press(KeyCode::Char('/')));
        assert!(
            app.entries.filter.is_open,
            "Slash should open the filter overlay"
        );

        app.handle_key(&press(KeyCode::Char('q')));
        assert!(
            !app.should_quit,
            "Typing 'q' in the search bar should not quit the app!"
        );

        app.handle_key(&press(KeyCode::Esc));
        assert!(!app.entries.filter.is_open, "Esc should close the filter");

        app.handle_key(&press(KeyCode::Char('h')));
        assert!(app.show_help, "'h' should open the help menu");

        app.handle_key(&press(KeyCode::Esc));
        assert!(!app.show_help, "Esc should close the help menu");

        app.handle_key(&press(KeyCode::Char('q')));
        assert!(
            app.should_quit,
            "'q' should quit the app when nothing is actively focused"
        );
    }
}
