//! A selection in the diff: it lives on one side and runs between two positions (line and byte offset into that
//! line's text); a double click takes a word (`word_at`).

use crate::review::Review;
pub use artoo_core::select::{Pos, Sel, word_at};

impl Review {
    /// The selected text, `None` when nothing is selected.
    pub fn selected_text(&self) -> Option<String> {
        let sel = self.sel.filter(|s| !s.is_empty())?;
        let data = self.data.as_ref()?;
        let side = data.side(sel.old);
        let (start, end) = sel.ends();
        let lines: Vec<&str> = (start.line..=end.line)
            .map(|n| {
                let text = side.line(n).0;
                let from = if n == start.line {
                    start.byte.min(text.len())
                } else {
                    0
                };
                let to = if n == end.line {
                    end.byte.min(text.len())
                } else {
                    text.len()
                };
                text.get(from..to).unwrap_or_default()
            })
            .collect();
        Some(lines.join("\n"))
    }

    pub fn line_text(&self, old: bool, line: u32) -> String {
        self.data
            .as_ref()
            .map(|d| d.side(old).line(line).0.to_owned())
            .unwrap_or_default()
    }
}
