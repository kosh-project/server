use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    style::{Color, Style, Stylize},
    text::Span,
};
use webdav_server::logger::{Level, Module};

mod selected;
use selected::Selected;
mod render;

use crate::entry::Entry;

#[derive(Default)]
pub struct Filter {
    module: Option<Module>,
    level: Option<Level>,
    pattern: String,
    raw_pattern: String,
    pub selected: Selected,
    pub is_open: bool,
}

impl Filter {
    pub(crate) const fn is_active(&self) -> bool {
        self.module.is_some()
            || self.level.is_some()
            || !self.pattern.is_empty()
    }

    #[inline]
    pub(crate) fn matches(&self, entry: &Entry) -> bool {
        if let Some(module) = self.module
            && module != entry.raw.module
        {
            return false;
        }

        if let Some(level) = self.level
            && level != entry.raw.level
        {
            return false;
        }

        if !self.raw_pattern.is_empty() {
            return entry.raw.message.contains(self.raw_pattern.as_str());
        }

        true
    }

    pub fn handle_key(&mut self, key_event: &KeyEvent) -> bool {
        match key_event.code {
            KeyCode::Tab => self.selected = self.selected.next(),
            KeyCode::BackTab => self.selected = self.selected.prev(),
            KeyCode::Char(' ') => {
                self.handle_space();
                return true;
            }
            KeyCode::Char(x) if Selected::Pattern == self.selected => {
                self.pattern.push(x);
                self.raw_pattern.push(x.to_ascii_lowercase());
                return true;
            }
            KeyCode::Backspace if Selected::Pattern == self.selected => {
                self.raw_pattern.pop();
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
            Selected::Pattern => {
                self.pattern.push(' ');
                self.raw_pattern.push(' ');
            }
        }
    }

    pub fn help_line(&self) -> Vec<Span<'_>> {
        match self.selected {
            Selected::Module => vec![
                "<Esc> ".blue().bold(),
                "Hide Filters".into(),
                " | ".yellow(),
                "<h> ".blue().bold(),
                "Help".into(),
                " | ".yellow(),
                "<TAB> ".blue().bold(),
                "Switch Filter".into(),
                " | ".yellow(),
                "<SPACE> ".blue().bold(),
                "Switch Module".into(),
            ],
            Selected::Level => vec![
                "<Esc> ".blue().bold(),
                "Hide Filters".into(),
                " | ".yellow(),
                "<h> ".blue().bold(),
                "Help".into(),
                " | ".yellow(),
                "<TAB> ".blue().bold(),
                "Switch Filter".into(),
                " | ".yellow(),
                "<SPACE> ".blue().bold(),
                "Switch Level".into(),
            ],
            Selected::Pattern => vec![
                "<Esc> ".blue().bold(),
                "Hide Filters".into(),
                " | ".yellow(),
                "<TAB> ".blue().bold(),
                "Switch Filter".into(),
            ],
        }
    }
}

const fn next_lvl(lvl: Option<Level>) -> Option<Level> {
    use Level::{Error, Fatal, Info, Warning};
    match lvl {
        Some(Error) => Some(Warning),
        Some(Warning) => Some(Fatal),
        Some(Fatal) => Some(Info),
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
const fn lvl_str(lvl: Option<Level>) -> &'static str {
    match lvl {
        Some(Level::Info) => "Info",
        Some(Level::Warning) => "Warning",
        Some(Level::Error) => "Error",
        Some(Level::Fatal) => "Fatal",
        Some(Level::Shutdown) => "",
        None => "ALL",
    }
}
