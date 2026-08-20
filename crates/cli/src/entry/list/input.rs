use std::cmp;

use crossterm::event::{
    KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind,
};

use crate::entry::List;

impl List {
    pub fn handle_filter(&mut self, event: &KeyEvent) {
        if self.filter.handle_key(event) {
            self.apply_filter();
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.scroll_up(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_down(1),
            KeyCode::PageUp => self.scroll_up(10),
            KeyCode::PageDown => self.scroll_down(10),
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, mouse: &MouseEvent) {
        let area = self.area;

        let border_level_x = area.x + self.col_level_w;
        // let border_module_x = border_level_x + 1 + self.col_module_w;
        let border_time_x =
            area.x + area.width.saturating_sub(self.col_level_w);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if mouse.column >= border_level_x.saturating_sub(1)
                    && mouse.column <= border_level_x + 1
                {
                    self.dragging_col = Some(1);
                } else if mouse.column >= border_time_x.saturating_sub(1)
                    && mouse.column <= border_time_x + 1
                {
                    self.dragging_col = Some(2);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(col) = self.dragging_col {
                    match col {
                        0 => {
                            self.col_level_w =
                                cmp::max(4, mouse.column.saturating_sub(area.x))
                        }
                        1 => {
                            let new_w =
                                mouse.column.saturating_sub(border_level_x + 1);
                            self.col_module_w = cmp::max(5, new_w);
                        }
                        2 => {
                            let new_w = (area.x + area.width)
                                .saturating_sub(mouse.column);
                            self.col_time_w = cmp::max(5, new_w);
                        }
                        _ => {}
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => self.dragging_col = None,
            MouseEventKind::ScrollUp => self.scroll_up(2),
            MouseEventKind::ScrollDown => self.scroll_down(2),
            _ => {}
        }
    }
}
