use std::cmp;

use crate::entry::List;

impl List {
    pub fn scroll_up(&mut self, mut steps: u16) {
        self.auto_scroll = false;
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
            if self.auto_scroll {
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
            self.scroll_to_bottom();
        }

        if self.is_at_bottom() {
            self.auto_scroll = true;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.logs.is_empty() || self.last_viewport_h == 0 {
            return;
        }

        let list_len = self.active_len();

        if list_len == 0 {
            self.top_log_idx = 0;
            self.top_log_line_offset = 0;
            return;
        }

        self.top_log_idx = list_len - 1;
        let last_log = if self.filter.is_active() {
            let idx = self.filtered_list.back().unwrap() - self.base_id;
            &self.logs[idx as usize]
        } else {
            self.logs.back().unwrap()
        };
        let height = self.calculate_height(&last_log.raw.message);
        self.top_log_line_offset = height.saturating_sub(1);

        self.scroll_up(self.last_viewport_h.saturating_sub(1));
        self.auto_scroll = true;
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

    #[inline]
    fn active_len(&self) -> usize {
        if self.filter.is_active() {
            self.filtered_list.len()
        } else {
            self.logs.len()
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_height_math() {
        let list = List {
            last_msg_col_w: 10,
            ..Default::default()
        };

        assert_eq!(list.calculate_height(""), 0);

        assert_eq!(list.calculate_height("short"), 1);

        assert_eq!(list.calculate_height("1234567890"), 2);

        assert_eq!(list.calculate_height("line1\nline2\nline3"), 3);

        assert_eq!(list.calculate_height("short\nand a long message"), 3);
    }

    #[test]
    fn test_viewport_scrolling_simulation() {
        use crate::entry::Entry;
        use webdav_server::logger::{Entry as LogEntry, Level, Module};

        let mut list = List::default();
        list.last_viewport_h = 5;
        list.last_msg_col_w = 100;
        for _ in 0..10 {
            let log = LogEntry {
                timestamp_ms: 0,
                module: Module::Server,
                level: Level::Info,
                message: "A".to_string(),
            };
            list.logs.push_back(Entry::from(log));
        }

        list.scroll_to_bottom();
        assert!(
            list.auto_scroll,
            "Snapping to bottom should engage the auto_scroll lock"
        );

        assert_eq!(list.top_log_idx, 5);
        assert_eq!(list.top_log_line_offset, 0);

        list.scroll_up(2);
        assert!(
            !list.auto_scroll,
            "Manual scrolling up MUST break the auto_scroll lock!"
        );

        assert_eq!(list.top_log_idx, 3);

        list.scroll_down(2);
        assert_eq!(list.top_log_idx, 5);

        list.scroll_down(100);

        assert_eq!(list.top_log_idx, 5);
        assert!(
            list.auto_scroll,
            "Smashing the bottom boundary should re-engage auto_scroll!"
        );
    }
}
