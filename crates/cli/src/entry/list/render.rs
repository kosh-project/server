use std::cmp;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::Span,
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Widget, Wrap},
};

use crate::entry::{self, Entry};

impl entry::List {
    fn show_logs(&self, area: Rect, buf: &mut Buffer) {
        let iter = self.logs.iter().skip(self.top_log_idx);

        self.draw_logs(iter, area, buf);
    }

    fn show_filtered(&self, area: Rect, buf: &mut Buffer) {
        let iter = self
            .filtered_list
            .iter()
            .skip(self.top_log_idx)
            .map(|x| &self.logs[(*x - self.base_id) as usize]);

        self.draw_logs(iter, area, buf);
    }

    fn draw_logs<'a>(
        &self,
        visible_logs: impl Iterator<Item = &'a Entry>,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let mut current_y = area.y;
        let mut is_first = true;
        for log in visible_logs {
            let log_height = self.calculate_height(&log.raw.message);

            let hidden_lines = if is_first {
                is_first = false;
                self.top_log_line_offset
            } else {
                0
            };

            let hidden_lines =
                cmp::min(hidden_lines, log_height.saturating_sub(1));

            let rect = Rect {
                x: area.x,
                y: current_y,
                width: area.width,
                height: log_height - hidden_lines,
            };

            let visible_rect = rect.intersection(area);
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

            let msg = Span::raw(log.msg.as_str());
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

impl Widget for &mut entry::List {
    fn render(self, area: Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let block = Block::default()
            .title(">  System Logs  |")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .padding(Padding::new(2, 2, 1, 0))
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

        if self.auto_scroll {
            self.scroll_to_bottom();
        }

        if self.filter.is_active() {
            self.show_filtered(inner_area, buf);
        } else {
            self.show_logs(inner_area, buf);
        }
    }
}
