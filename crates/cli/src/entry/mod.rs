use chrono::{DateTime, Local, Timelike};
use ratatui::{
    style::{Color, Style},
    text::Span,
};

mod filter;
mod list;
mod ui;

pub use filter::Filter;

pub use list::EntryList as List;

pub struct Entry {
    pub level: Span<'static>,
    pub module: Span<'static>,
    pub time: Span<'static>,
    pub msg: String,
    pub raw: logger::Entry,
}

use webdav_server::logger::{self, Level, Module};

const GREEN: Style = Style::new().fg(Color::Green);
const RED: Style = Style::new().fg(Color::Red);
const YELLOW: Style = Style::new().fg(Color::Yellow);
const GRAY: Style = Style::new().fg(Color::Gray);
const LIGHT_RED: Style = Style::new().fg(Color::LightRed);

fn level_into_span(value: Level) -> Span<'static> {
    use Level::{Error, Fatal, Info, Shutdown, Warning};

    match value {
        Info => Span::styled("INFO", GRAY),
        Warning => Span::styled("WARNING", YELLOW),
        Error => Span::styled("ERROR", LIGHT_RED),
        Fatal => Span::styled("FATAL", RED),
        Shutdown => Span::styled("SHUTDOWN", GRAY),
    }
}

fn module_into_span(value: Module) -> Span<'static> {
    use Module::{Api, Asset, Database, Logger, Server, Storage};

    match value {
        Api => Span::styled("Api", GREEN),
        Database => Span::styled("Database", GREEN),
        Server => Span::styled("Server", GREEN),
        Asset => Span::styled("Asset", GREEN),
        Storage => Span::styled("Storage", GREEN),
        Logger => Span::styled("Logger", YELLOW),
    }
}

fn timestamp_millis_span(timestamp_ms: i64) -> Span<'static> {
    let time = DateTime::from_timestamp_millis(timestamp_ms)
        .unwrap()
        .with_timezone(&Local);

    let time = format!(
        "{:02}:{:02}:{:02}",
        time.hour(),
        time.minute(),
        time.second(),
    );

    Span::styled(time, GRAY)
}

impl From<logger::Entry> for Entry {
    fn from(mut entry: logger::Entry) -> Self {
        let search_index = entry.message.to_ascii_lowercase();
        let message = entry.message;

        entry.message = search_index;

        Self {
            level: level_into_span(entry.level),
            module: module_into_span(entry.module),
            time: timestamp_millis_span(entry.timestamp_ms),
            msg: message,

            raw: entry,
        }
    }
}
