//! The find over the diff (⌘F): the rows that hold the text and each change's start, stepping through them in
//! the open file or in every file of the one scroll, the rows brought into view.

use crate::review::Review;
use artoo_core::rows::Row;
use artoo_core::{rows, search};
use artoo_ui_kit::px;
use std::sync::Arc;

impl Review {
    /// The rows of the open diff that hold the find text, and where each change begins.
    pub fn recompute_matches(&mut self) {
        self.matches = match &self.data {
            Some(data) => rows::matching_rows(data, &self.rows, &self.dterms),
            None => Vec::new(),
        };
        self.match_at = self.match_at.min(self.matches.len().saturating_sub(1));
        let mut changes = Vec::new();
        let mut inside = false;
        for (ix, row) in self.rows.iter().enumerate() {
            match row {
                Row::Thread(_) | Row::Request(_) | Row::Draft(_) | Row::Note(_) | Row::Composer => {
                    continue;
                }
                row if rows::is_change(row) => {
                    if !inside {
                        changes.push(ix);
                    }
                    inside = true;
                }
                _ => inside = false,
            }
        }
        self.changes = changes;
        self.change_at = self.change_at.filter(|&at| at < self.changes.len());
        self.recompute_stack_matches();
    }

    /// Scrolls the diff so `row` sits a few lines below the top.
    pub fn scroll_to_row(&mut self, row: usize) {
        self.open_active();
        let row = row + self.list_base();
        self.diff_list.scroll_to(gpui::ListOffset {
            item_ix: row.saturating_sub(3),
            offset_in_item: px(0.),
        });
    }

    /// Brings `rows` into view when they are not, the first a third of the view from the top; rows
    /// already in view stay where they are. The view is counted in code rows: right after a
    /// relayout the list has not measured what lies below, so `scroll_to_reveal_item` cannot reach
    /// it.
    pub fn reveal_rows(&mut self, rows: std::ops::Range<usize>) {
        self.open_active();
        let base = self.list_base();
        let rows = rows.start + base..rows.end + base;
        let height = f32::from(self.diff_list.viewport_bounds().size.height);
        let fit = (height / f32::from(px(self.row_height()))).floor() as usize;
        let top = self.diff_list.logical_scroll_top().item_ix;
        // The top row may be cut, and threads are taller than a code row: keep a margin.
        if fit > 6 && rows.start > top && rows.end + 3 <= top + fit {
            return;
        }
        self.diff_list.scroll_to(gpui::ListOffset {
            item_ix: rows.start.saturating_sub(fit / 3),
            offset_in_item: px(0.),
        });
    }

    /// With every file in one scroll: the match of every file's after (or before) the current one.
    pub fn next_stack_match(&self, forward: bool) -> Option<usize> {
        let stack = self.stack.as_ref()?;
        let n = stack.matches.len();
        (n > 0).then(|| {
            if forward {
                (stack.match_at + 1) % n
            } else {
                (stack.match_at + n - 1) % n
            }
        })
    }

    /// The open file's next (or previous) match comes into view; false when it has none.
    pub fn step_match(&mut self, forward: bool) -> bool {
        let n = self.matches.len();
        if n == 0 {
            return false;
        }
        self.match_at = if forward {
            (self.match_at + 1) % n
        } else {
            (self.match_at + n - 1) % n
        };
        self.scroll_to_row(self.matches[self.match_at]);
        true
    }

    /// The open file's next (or previous) change comes into view; false when it has none.
    pub fn step_change(&mut self, forward: bool) -> bool {
        let n = self.changes.len();
        if n == 0 {
            return false;
        }
        let at = match (self.change_at, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(at), true) => (at + 1) % n,
            (Some(at), false) => (at + n - 1) % n,
        };
        self.change_at = Some(at);
        self.scroll_to_row(self.changes[at]);
        true
    }

    /// Find in the diff closes.
    pub fn close_find(&mut self) {
        self.dfind = None;
        self.dterms = Arc::default();
        self.recompute_matches();
    }

    /// Find in the diff looks for `text` (as typed, or as a regular expression); in the open file the
    /// first match comes into view.
    pub fn set_find(&mut self, text: String) {
        let terms = if self.dfind_regex {
            search::Terms::regex(&text)
        } else {
            Ok(search::Terms::text(&text))
        };
        self.dfind_error = terms.as_ref().err().cloned();
        self.dterms = Arc::new(terms.unwrap_or_default());
        self.dfind = Some(text);
        self.match_at = 0;
        self.recompute_matches();
        if self.stack.is_none()
            && let Some(&row) = self.matches.first()
        {
            self.scroll_to_row(row);
        }
    }
}
