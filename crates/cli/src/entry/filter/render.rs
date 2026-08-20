use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    text::Span,
    widgets::{Block, BorderType, Borders, Widget},
};

use crate::entry::{
    Filter,
    filter::{lvl_str, module_str, span_style},
};

impl Widget for &Filter {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let block = Block::new()
            .title("> Filter Logs |")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let inner_area = block.inner(area);
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(9),
                Constraint::Fill(1),
                Constraint::Length(2),
                Constraint::Length(20),
                Constraint::Length(2),
                Constraint::Length(20),
            ])
            .split(inner_area);

        let search_span =
            Span::styled("Search :", span_style(self.selected.is_pattern()));

        let pattern_length = self.pattern.chars().count();

        let pattern = if layout[1].width as usize >= pattern_length {
            self.pattern.as_str()
        } else {
            &self.pattern[pattern_length - layout[1].width as usize..]
        };

        let module_span = Span::styled(
            format!("Module : {}", module_str(self.module)),
            span_style(self.selected.is_module()),
        );

        let level_span = Span::styled(
            format!("Level : {}", lvl_str(self.level)),
            span_style(self.selected.is_level()),
        );

        block.render(area, buf);

        "|".render(layout[2], buf);
        "|".render(layout[4], buf);

        pattern.render(layout[1], buf);
        module_span.render(layout[3], buf);
        search_span.render(layout[0], buf);
        level_span.render(layout[5], buf);
    }
}
