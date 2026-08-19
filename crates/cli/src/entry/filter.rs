use std::{env::SplitPaths, net::Shutdown};

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Widget},
};
use webdav_server::logger::{Entry, Level, Module};

#[derive(Default)]
pub struct Filter {
    module: Option<Module>,
    level: Option<Level>,
    pattern: String,
    pub selected: Selected,
    pub is_open: bool,
}

#[derive(PartialEq)]
pub enum Selected {
    Module,
    Level,
    Pattern,
}

impl Selected {
    const fn next(&self) -> Self {
        match self {
            Self::Level => Self::Module,
            Self::Module => Self::Pattern,
            Self::Pattern => Self::Level,
        }
    }

    const fn prev(&self) -> Self {
        match self {
            Self::Level => Self::Pattern,
            Self::Pattern => Self::Module,
            Self::Module => Self::Level,
        }
    }

    const fn is_pattern(&self) -> bool {
        matches!(self, Self::Pattern)
    }

    const fn is_module(&self) -> bool {
        matches!(self, Self::Module)
    }

    const fn is_level(&self) -> bool {
        matches!(self, Self::Level)
    }
}

impl Default for Selected {
    #[inline]
    fn default() -> Self {
        return Self::Pattern;
    }
}

impl Filter {
    pub(crate) const fn is_active(&self) -> bool {
        return self.module.is_some()
            || self.level.is_some()
            || !self.pattern.is_empty();
    }

    #[inline]
    pub(crate) fn matches(&self, entry: &Entry) -> bool {
        if let Some(module) = self.module {
            if module != entry.module {
                return false;
            }
        }
        if let Some(level) = self.level {
            if level != entry.level {
                return false;
            }
        }

        if !self.pattern.is_empty() {
            return entry.message.contains(self.pattern.as_str());
        }

        return true;
    }

    pub fn handle_key(&mut self, key_event: &KeyEvent) -> bool {
        // todo!(
        //     "Make this function return a bool, so that you know, if key event tweaks a filter"
        // );
        match key_event.code {
            KeyCode::Tab => self.selected = self.selected.next(),
            KeyCode::BackTab => self.selected = self.selected.prev(),
            KeyCode::Char(' ') => {
                self.handle_space();
                return true;
            }
            KeyCode::Char(x) if Selected::Pattern == self.selected => {
                self.pattern.push(x);
                return true;
            }
            KeyCode::Backspace if Selected::Pattern == self.selected => {
                return self.pattern.pop().is_some();
            }
            _ => {}
        }
        false
    }

    fn handle_space(&mut self) {
        match self.selected {
            Selected::Level => self.level = next_lvl(self.level),
            Selected::Module => self.module = next_mod(self.module),
            Selected::Pattern => self.pattern.push(' '),
        }
    }

    pub fn help_line(&self) -> Vec<Span> {
        match self.selected {
            Selected::Module => vec![
                " <h> -".blue().bold(),
                " Help ".into(),
                " <TAB> -".blue().bold(),
                " Switch Filter ".into(),
                " <SPACE> -".blue().bold(),
                " Switch Module ".into(),
            ],
            Selected::Level => vec![
                " <h> -".blue().bold(),
                " Help ".into(),
                " <TAB> -".blue().bold(),
                " Switch Filter ".into(),
                " <SPACE> -".blue().bold(),
                " Switch Level ".into(),
            ],
            Selected::Pattern => {
                vec![
                    " <h> -".blue().bold(),
                    " Help ".into(),
                    " <TAB> -".blue().bold(),
                    " Switch Filter ".into(),
                ]
            }
        }
    }
}

const fn next_lvl(lvl: Option<Level>) -> Option<Level> {
    use Level::{Error, Fatal, Info, Warning};
    match lvl {
        Some(Error) => Some(Warning),
        Some(Warning) => Some(Fatal),
        Some(Fatal) => Some(Info),
        Some(Info) => None,
        None => Some(Error),
        Some(_) => None, // Shutdown Module Can't be logged
    }
}

const fn next_mod(module: Option<Module>) -> Option<Module> {
    use Module::{Api, Asset, Database, Logger, Server, Storage};
    match module {
        Some(Api) => Some(Database),
        Some(Database) => Some(Server),
        Some(Server) => Some(Asset),
        Some(Asset) => Some(Storage),
        Some(Storage) => Some(Logger),
        Some(Logger) => None,
        None => Some(Api),
    }
}

const fn span_style(selected: bool) -> Style {
    if selected {
        Style::new().fg(Color::Yellow).bold()
    } else {
        Style::new()
    }
}

#[inline]
const fn module_str(module: Option<Module>) -> &'static str {
    match module {
        Some(Module::Api) => "Api",
        Some(Module::Database) => "Database",
        Some(Module::Server) => "Server",
        Some(Module::Asset) => "Asset",
        Some(Module::Storage) => "Storage",
        Some(Module::Logger) => "Logger",
        None => "ALL",
    }
}

#[inline]
const fn lvl_str(module: Option<Level>) -> &'static str {
    match module {
        Some(Level::Info) => "Info",
        Some(Level::Warning) => "Warning",
        Some(Level::Error) => "Error",
        Some(Level::Fatal) => "Fatal",
        Some(Level::Shutdown) => "",
        None => "ALL",
    }
}

impl Widget for &Filter {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let block = Block::new()
            .title("Filter Logs")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let inner_area = block.inner(area);
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(2),
                Constraint::Length(15),
                Constraint::Length(2),
                Constraint::Length(20),
            ])
            .split(inner_area);

        let pattern_span = Span::styled(
            format!("Search : {}_", self.pattern),
            span_style(self.selected.is_pattern()),
        );

        let module_span = Span::styled(
            format!("Module : {}", module_str(self.module)),
            span_style(self.selected.is_module()),
        );

        let level_span = Span::styled(
            format!("Level : {}", lvl_str(self.level)),
            span_style(self.selected.is_level()),
        );

        block.render(area, buf);

        module_span.render(layout[2], buf);
        pattern_span.render(layout[0], buf);
        level_span.render(layout[4], buf);
    }
}
