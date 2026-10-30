//! A selection in the diff: it lives on one side and runs between two positions (line and byte offset into that
//! line's text); a double click takes a word (`word_at`).

use artoo_core::worddiff;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    /// 1-based.
    pub line: u32,
    /// Byte offset into the line's text.
    pub byte: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel {
    /// On the removed side of the diff.
    pub old: bool,
    pub anchor: Pos,
    pub head: Pos,
}

impl Sel {
    /// The two ends, first then last.
    pub fn ends(&self) -> (Pos, Pos) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// What of a line of `len` bytes is selected; `None` when none of it is.
    pub fn in_line(&self, line: u32, len: usize) -> Option<Range<usize>> {
        let (start, end) = self.ends();
        if self.is_empty() || line < start.line || line > end.line {
            return None;
        }
        let from = if line == start.line {
            start.byte.min(len)
        } else {
            0
        };
        let to = if line == end.line {
            end.byte.min(len)
        } else {
            len
        };
        (from < to).then_some(from..to)
    }
}

/// The word, run of spaces or single symbol at `byte` of `text`.
pub fn word_at(text: &str, byte: usize) -> Range<usize> {
    let words = worddiff::words(text);
    words
        .iter()
        .find(|w| w.start <= byte && byte < w.end)
        .or_else(|| words.last().filter(|w| w.end == byte))
        .cloned()
        .unwrap_or(byte..byte)
}
