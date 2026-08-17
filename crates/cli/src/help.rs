use std::sync::OnceLock;

use ratatui::{
    layout::HorizontalAlignment,
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Widget},
};

pub struct Help;

const HOTKEY: Style =
    Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD);

static HELP_BODY: OnceLock<Vec<Line<'static>>> = OnceLock::new();

fn help_body() -> &'static [Line<'static>] {
    HELP_BODY.get_or_init(|| {
        vec![
            Line::from(vec![
                Span::styled("[↑/↓]          :", HOTKEY),
                Span::raw(" Scroll Logs"),
            ]),
            Line::from(vec![
                Span::styled("[PgUp/PgDown]  :", HOTKEY),
                Span::raw(" Faster Scroll:"),
            ]),
            Line::from(vec![
                Span::styled("[MouseDrag]    :", HOTKEY),
                Span::raw(" Resize columns"),
            ]),
            Line::from(vec![
                Span::styled("[q]            :", HOTKEY),
                Span::raw(" Quit"),
            ]),
            Line::from(vec![
                Span::styled("[h]            :", HOTKEY),
                Span::raw(" Toggle help"),
            ]),
        ]
    })
}

impl Widget for Help {
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
    ) where
        Self: Sized,
    {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("|  Hotkey Menu  |")
            .padding(Padding::new(2, 2, 1, 1))
            .fg(Color::Green)
            .title_alignment(HorizontalAlignment::Center);

        let inner_area = block.inner(area);

        let help_body = help_body();

        Clear.render(area, buf);

        block.render(area, buf);

        for (i, line) in help_body.iter().enumerate() {
            if i as u16 >= inner_area.height {
                break;
            }

            buf.set_line(
                inner_area.x,
                inner_area.y + i as u16,
                line,
                inner_area.width,
            );
        }
    }
}
