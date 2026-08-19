use std::{cmp, collections::VecDeque};

use bincode_next::config::{self, Config};
use crossterm::event::{
    KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use webdav_server::logger;

use crate::entry::{Entry, Filter};

pub struct EntryList {
    pub logs: VecDeque<Entry>,

    pub top_log_idx: usize,
    pub top_log_line_offset: u16,

    pub last_msg_col_w: u16,
    pub last_viewport_h: u16,

    pub col_level_w: u16,
    pub col_module_w: u16,
    pub col_time_w: u16,

    pub base_id: u64,

    pub force_scroll_to_bottom: bool,

    pub area: Rect,

    pub dragging_col: Option<usize>,

    pub filter: Filter,
    pub filtered_list: VecDeque<u64>,
    pub raw: VecDeque<logger::Entry>,
}

impl Default for EntryList {
    fn default() -> Self {
        Self {
            logs: VecDeque::with_capacity(3000),
            top_log_idx: 0,
            top_log_line_offset: 0,
            last_msg_col_w: 10,
            last_viewport_h: 10,
            col_level_w: 7,
            base_id: 0,
            col_module_w: 12,
            col_time_w: 10,
            dragging_col: None,
            force_scroll_to_bottom: true,
            area: Rect::default(),
            filter: Filter::default(),
            filtered_list: VecDeque::with_capacity(3000),
            raw: VecDeque::with_capacity(3000),
        }
    }
}

impl EntryList {
    pub(crate) fn init_or_default(mut bytes: &[u8]) -> Self {
        let mut list = Self::default();

        while !bytes.is_empty()
            && let Ok((entry, bytes_read)) =
                bincode_next::decode_from_slice::<logger::Entry, _>(
                    bytes,
                    config::standard(),
                )
        {
            list.raw.push_back(entry.clone());

            let entry = Entry::from(entry);
            list.logs.push_back(entry);

            bytes = &bytes[bytes_read..];
        }

        if !bytes.is_empty() {
            list = EntryList::default();
        }

        list.scroll_to_bottom();

        list
    }

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

    pub fn scroll_up(&mut self, mut steps: u16) {
        while steps > 0 {
            if self.top_log_line_offset > 0 {
                let scroll_amount = cmp::min(steps, self.top_log_line_offset);
                self.top_log_line_offset -= scroll_amount;
                steps -= scroll_amount;
            } else if self.top_log_idx > 0 {
                self.top_log_idx -= 1;
                self.top_log_line_offset = self
                    .calculate_height(&self.logs[self.top_log_idx].raw.message)
                    .saturating_sub(1);

                steps -= 1;
            } else {
                break;
            }
        }
    }

    pub fn scroll_down(&mut self, steps: u16) {
        if self.logs.is_empty() {
            return;
        }

        for _ in 0..steps {
            if self.is_at_bottom() {
                break;
            }

            if self.top_log_idx >= self.logs.len() {
                break;
            }

            self.top_log_line_offset += 1;

            if self.top_log_line_offset
                >= self
                    .calculate_height(&self.logs[self.top_log_idx].raw.message)
            {
                self.top_log_idx += 1;
                self.top_log_line_offset = 0;
            }
        }

        if self.top_log_idx >= self.logs.len() {
            self.top_log_idx = self.logs.len().saturating_sub(1);
            self.top_log_line_offset = 0;
        }
    }

    pub fn is_at_bottom(&self) -> bool {
        if self.logs.is_empty() {
            return true;
        }

        let mut total_hieght = 0;
        let mut is_first = true;
        for log in self.logs.iter().skip(self.top_log_idx) {
            let h = self.calculate_height(&log.raw.message);
            let hidden = if is_first {
                self.top_log_line_offset
            } else {
                0
            };
            is_first = false;

            total_hieght += h.saturating_sub(hidden);
            if total_hieght > self.last_viewport_h {
                return false;
            }
        }
        true
    }

    pub fn calculate_height(&self, msg: &str) -> u16 {
        if self.last_msg_col_w == 0 {
            return 1;
        }
        let mut total = 0;
        for line in msg.lines() {
            let len = line.len() as u16;
            total += (len / self.last_msg_col_w) + 1;
        }
        total
    }

    pub fn handle_mouse(&mut self, mouse: &MouseEvent) {
        let area = self.area;

        let border_level_x = area.x + self.col_level_w;
        let border_module_x = border_level_x + 1 + self.col_module_w;
        let border_time_x =
            area.x + area.width.saturating_sub(self.col_level_w);

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if mouse.column >= border_level_x.saturating_sub(1)
                    && mouse.column <= border_level_x + 1
                {
                    self.dragging_col = Some(1);
                } else if mouse.column >= border_module_x.saturating_sub(1)
                    && mouse.column <= border_module_x + 1
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

    pub fn apply_filter(&mut self) {
        self.filtered_list.clear();

        if !self.filter.is_active() {
            return;
        }

        for (idx, log) in self.raw.iter().enumerate() {
            if self.filter.matches(log) {
                self.filtered_list.push_back(self.base_id + idx as u64);
            }
        }

        self.scroll_to_bottom();
    }

    pub fn add_log(&mut self, entry: logger::Entry) {
        let at_bottom = self.is_at_bottom();

        if self.logs.len() > 2999 {
            self.logs.pop_front();
            self.raw.pop_front();
            self.base_id += 1;

            if let Some(&first_id) = self.filtered_list.front() {
                if first_id < self.base_id {
                    self.filtered_list.pop_front();
                }
            }
        }

        let new_id = self.base_id + self.logs.len() as u64;
        self.logs.push_back(Entry::from(entry.clone()));
        self.raw.push_back(entry);

        if self.filter.is_active()
            && self.filter.matches(self.raw.back().unwrap())
        {
            self.filtered_list.push_back(new_id);
        }

        if at_bottom {
            self.scroll_to_bottom();
        }
    }

    pub fn active_log(&self, idx: usize) -> &Entry {
        if self.filter.is_active() {
            let target_id = self.filtered_list[idx];

            let idx = (target_id - self.base_id) as usize;

            &self.logs[idx]
        } else {
            &self.logs[idx]
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.logs.is_empty() || self.last_viewport_h == 0 {
            return;
        }

        self.top_log_idx = self.logs.len() - 1;
        let height =
            self.calculate_height(&self.logs.back().unwrap().raw.message);
        self.top_log_line_offset = height.saturating_sub(1);

        self.scroll_up(self.last_viewport_h.saturating_sub(1));
    }
}
