use std::cmp;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Widget, Wrap},
};

use crate::list::EntryList;

impl Widget for &mut EntryList {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let block = Block::default()
            .title(">  System Logs  |")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .padding(Padding::new(2, 2, 1, 1))
            .border_style(Style::default());

        let inner_area = block.inner(area);
        self.area = inner_area;
        block.render(area, buf);

        if inner_area.width == 0 || inner_area.height == 0 {
            return;
        }

        let msg_width = inner_area.width.saturating_sub(
            self.col_level_w + self.col_module_w + self.col_time_w + 3,
        );

        self.last_msg_col_w = msg_width;
        self.last_viewport_h = inner_area.height;

        let visible_logs = self.logs.iter().skip(self.top_log_idx);
        let mut current_y = inner_area.y;
        let mut is_first = true;

        for log in visible_logs {
            let log_height = self.calculate_height(&log.message);

            let hidden_lines = if is_first {
                is_first = false;
                self.top_log_line_offset
            } else {
                0
            };

            let hidden_lines =
                cmp::min(hidden_lines, log_height.saturating_sub(1));

            let rect = Rect {
                x: inner_area.x,
                y: current_y,
                width: inner_area.width,
                height: log_height - hidden_lines,
            };

            let visible_rect = rect.intersection(inner_area);
            if visible_rect.width == 0 || visible_rect.height == 0 {
                break;
            }

            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .spacing(2)
                .constraints([
                    Constraint::Length(self.col_level_w),
                    Constraint::Length(self.col_module_w),
                    Constraint::Fill(1),
                    Constraint::Length(self.col_time_w),
                ])
                .split(visible_rect);

            buf.set_span(
                columns[0].x,
                columns[0].y,
                &log.level,
                columns[0].width,
            );
            buf.set_span(
                columns[1].x,
                columns[1].y,
                &log.module,
                columns[1].width,
            );

            let msg = Span::raw(log.message.as_str());
            Paragraph::new(msg)
                .wrap(Wrap { trim: false })
                .scroll((hidden_lines, 0))
                .render(columns[2], buf);

            buf.set_span(
                columns[3].x,
                columns[3].y,
                &log.time,
                columns[3].width,
            );
            current_y += rect.height;
        }
    }
}
