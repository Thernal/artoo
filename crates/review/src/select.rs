//! A selection in the diff: it lives on one side and runs between two positions (line and byte offset into that
//! line's text); a double click takes a word (`word_at`).

use crate::review::Review;
pub use artoo_core::select::{Pos, Sel, word_at};

impl Review {
    /// A press on a line at byte `byte`: the caret, a word or the line, by click count; ⇧ extends.
    pub fn press(&mut self, old: bool, line: u32, byte: usize, clicks: usize, shift: bool) {
        let at = Pos { line, byte };
        let text = self.line_text(old, line);
        self.selecting = true;
        self.sel = match (self.sel, clicks, shift) {
            (Some(sel), 1, true) if sel.old == old => Some(Sel { head: at, ..sel }),
            (_, 2, _) => {
                let w = word_at(&text, byte);
                Some(Sel {
                    old,
                    anchor: Pos {
                        line,
                        byte: w.start,
                    },
                    head: Pos { line, byte: w.end },
                })
            }
            (_, n, _) if n >= 3 => Some(Sel {
                old,
                anchor: Pos { line, byte: 0 },
                head: Pos {
                    line,
                    byte: text.len(),
                },
            }),
            _ => Some(Sel {
                old,
                anchor: at,
                head: at,
            }),
        };
    }

    /// The pointer moved over a line with the button down: the selection reaches it. True when it moved.
    pub fn drag(&mut self, old: bool, line: u32, byte: usize) -> bool {
        let head = Pos { line, byte };
        if !self.selecting {
            return false;
        }
        if let Some(sel) = &mut self.sel
            && sel.old == old
            && sel.head != head
        {
            sel.head = head;
            return true;
        }
        false
    }

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
