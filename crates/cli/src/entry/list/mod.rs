use std::collections::VecDeque;

use bincode_next::config;
use ratatui::layout::Rect;
use webdav_server::logger;

mod input;
mod render;
mod scroll;

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

    /// Acts as an offset to keep absolute IDs for our logs.
    /// When the logs ring buffer fills up and we pop the oldest log,
    /// we increment base_id instead of having to shift all filtered indices.
    pub base_id: u64,

    // pub force_scroll_to_bottom: bool,
    pub area: Rect,

    pub dragging_col: Option<usize>,

    pub filter: Filter,

    /// Stores absolute IDs (base_id + physical index) of logs that match the current filter.
    /// This allows O(1) matching without re-evaluating the entire list when evicting old logs.
    pub filtered_list: VecDeque<u64>,

    /// Toggles the "tail-follow" mode. Set to false when the user manually scrolls up,
    /// and true when they reach the bottom of the log list.
    pub auto_scroll: bool,
}

const MAX_LOG_BUFFER: usize = 3000;

impl Default for EntryList {
    fn default() -> Self {
        Self {
            logs: VecDeque::with_capacity(MAX_LOG_BUFFER),
            top_log_idx: 0,
            top_log_line_offset: 0,
            last_msg_col_w: 10,
            last_viewport_h: 10,
            col_level_w: 7,
            base_id: 0,
            col_module_w: 12,
            col_time_w: 10,
            dragging_col: None,
            // force_scroll_to_bottom: true,
            area: Rect::default(),
            filter: Filter::default(),
            filtered_list: VecDeque::with_capacity(MAX_LOG_BUFFER),

            auto_scroll: true,
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
            let entry = Entry::from(entry);
            list.logs.push_back(entry);

            bytes = &bytes[bytes_read..];
        }

        if !bytes.is_empty() {
            list = Self::default();
        }

        list.scroll_to_bottom();

        list
    }

    pub fn apply_filter(&mut self) {
        self.filtered_list.clear();

        if !self.filter.is_active() {
            return;
        }

        for (idx, log) in self.logs.iter().enumerate() {
            if self.filter.matches(log) {
                self.filtered_list.push_back(self.base_id + idx as u64);
            }
        }

        self.scroll_to_bottom();
    }

    pub fn add_log(&mut self, entry: logger::Entry) {
        let at_bottom = self.auto_scroll;

        if self.logs.len() >= MAX_LOG_BUFFER {
            // Evict the oldest log to maintain a stable memory footprint.
            // We increment base_id so absolute IDs in the filtered list remain mathematically valid.
            self.logs.pop_front();
            self.base_id += 1;

            if let Some(&first_id) = self.filtered_list.front()
                && first_id < self.base_id
            {
                self.filtered_list.pop_front();
            }
        }

        let new_id = self.base_id + self.logs.len() as u64;
        self.logs.push_back(Entry::from(entry));

        if self.filter.is_active()
            && self.filter.matches(self.logs.back().as_ref().unwrap())
        {
            self.filtered_list.push_back(new_id);
        }

        if at_bottom {
            self.scroll_to_bottom();
        }
    }
}

#[cfg(test)]
mod tests {
    use webdav_server::logger::{Entry, Level, Module};

    use super::*;

    fn dummy_log(msg: &str, level: Level) -> Entry {
        Entry {
            timestamp_ms: 0,
            module: Module::Server,
            level,
            message: msg.to_owned(),
        }
    }

    #[test]
    fn entry_log_eviction_math() {
        let mut list = EntryList::default();

        for _ in 0..MAX_LOG_BUFFER {
            list.add_log(dummy_log("Very Normal Log", Level::Info));
        }

        assert_eq!(list.logs.len(), MAX_LOG_BUFFER);
        assert_eq!(list.base_id, 0);

        for _ in 0..5 {
            list.add_log(dummy_log("More logs!", Level::Info));
        }

        assert_eq!(list.logs.len(), MAX_LOG_BUFFER);
        assert_eq!(list.base_id, 5);
    }

    #[test]
    fn test_filtered_list_eviction() {
        let mut list = EntryList::default();

        list.filter.level = Some(Level::Error);

        list.add_log(dummy_log("Very Old Err", Level::Error));

        for _ in 0..(MAX_LOG_BUFFER - 1) {
            list.add_log(dummy_log("Normal Log", Level::Info));
        }

        assert_eq!(list.filtered_list.len(), 1);
        assert_eq!(list.filtered_list[0], 0);

        list.add_log(dummy_log("One More Log", Level::Info));

        assert_eq!(list.filtered_list.len(), 0);
    }

    #[test]
    fn perfect_payload_deserialization() {
        use bincode_next::config;
        use webdav_server::logger::{Entry as LogEntry, Level, Module};

        let log1 = LogEntry {
            timestamp_ms: 100,
            module: Module::Server,
            level: Level::Info,
            message: "Valid".to_string(),
        };
        let log2 = LogEntry {
            timestamp_ms: 200,
            module: Module::Api,
            level: Level::Error,
            message: "Corrupted".to_string(),
        };

        let bytes1 =
            bincode_next::encode_to_vec(&log1, config::standard()).unwrap();
        let bytes2 =
            bincode_next::encode_to_vec(&log2, config::standard()).unwrap();

        let mut perfect_payload = bytes1.clone();
        perfect_payload.extend(bytes2.iter());

        let list = EntryList::init_or_default(&perfect_payload);
        assert_eq!(
            list.logs.len(),
            2,
            "Failed to decode a perfect network payload"
        );

        assert_eq!(list.logs[0].raw.message, "valid");

        perfect_payload.truncate(perfect_payload.len() - 5);
        let corrupted = perfect_payload;

        let corrupted_list = EntryList::init_or_default(&corrupted);

        assert_eq!(
            corrupted_list.logs.len(),
            0,
            "Failed rejecting corruted network bytes!"
        );
    }
}
