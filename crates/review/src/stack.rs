//! Every file of a diff in one scroll: the diff's list holds a block per file, a header row and the file's rows
//! under it. The stack is the blocks and the list rows each one takes.

use crate::diff_view::LineNote;
use artoo_core::rows::{FileData, Row};
use gpui::{ListOffset, Task};
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

/// The ground between two files' cards.
pub const CARD_GAP: f32 = 16.;
/// The card's inset from the sides of the diff.
pub const CARD_MARGIN: f32 = 6.;
pub const CARD_RADIUS: f32 = 8.;

pub struct Block {
    pub file: usize,
    pub data: Option<Arc<FileData>>,
    pub rows: Vec<Row>,
    pub expanded: HashSet<usize>,
    pub notes: Vec<LineNote>,
    /// The rows the list holds under the header.
    pub len: usize,
}

#[derive(Default)]
pub struct Stack {
    pub blocks: Vec<Block>,
    /// The block of each file, by file index.
    of_file: Vec<Option<usize>>,
    /// The list index of each block's header; the list's length last.
    pub starts: Vec<usize>,
    /// Files folded or opened against the default (open; reviewed ones folded with View → Fold
    /// reviewed files).
    pub flipped: HashSet<usize>,
    pub building: bool,
    pub task: Option<Task<()>>,
    /// Where the view goes once the active file is laid out: rows below its header.
    pub anchor: Option<ListOffset>,
    /// The rows of every file that hold the find text, (file, row), in list order; the one
    /// stepped to.
    pub matches: Vec<(usize, usize)>,
    pub match_at: usize,
}

impl Stack {
    pub fn new(order: &[usize], files: usize) -> Self {
        let mut of_file = vec![None; files];
        let blocks = order
            .iter()
            .enumerate()
            .map(|(b, &file)| {
                of_file[file] = Some(b);
                Block {
                    file,
                    data: None,
                    rows: Vec::new(),
                    expanded: HashSet::new(),
                    notes: Vec::new(),
                    len: 0,
                }
            })
            .collect();
        let mut stack = Self {
            blocks,
            of_file,
            ..Default::default()
        };
        stack.reindex();
        stack
    }

    pub fn reindex(&mut self) {
        self.starts.clear();
        let mut at = 0;
        for block in &self.blocks {
            self.starts.push(at);
            at += 1 + block.len;
        }
        self.starts.push(at);
    }

    /// The list's length.
    pub fn total(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    /// The block of list row `ix`, and the row's place in it (0 is the header).
    pub fn locate(&self, ix: usize) -> (usize, usize) {
        let b = self
            .starts
            .partition_point(|&s| s <= ix)
            .saturating_sub(1)
            .min(self.blocks.len().saturating_sub(1));
        (b, ix.saturating_sub(self.starts[b]))
    }

    /// Whether list row `ix` is the last of its block.
    pub fn last_of_block(&self, ix: usize) -> bool {
        let (b, _) = self.locate(ix);
        ix + 1 == self.starts[b + 1]
    }

    pub fn block_of(&self, file: usize) -> Option<usize> {
        self.of_file.get(file).copied().flatten()
    }

    /// How many rows block `b` shows under its header (none while folded).
    pub fn shown(&self, b: usize) -> usize {
        self.starts[b + 1] - self.starts[b] - 1
    }

    /// The list index of file `file`'s header.
    pub fn header(&self, file: usize) -> Option<usize> {
        Some(self.starts[self.block_of(file)?])
    }

    /// The list rows of block `b`, header included.
    pub fn span(&self, b: usize) -> Range<usize> {
        self.starts[b]..self.starts[b + 1]
    }
}

/// What a file shows under its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Body {
    /// Nothing: the header alone.
    Folded,
    /// Its rows are being built.
    Waiting,
    /// One line about it: generated, an image, no content changes.
    Note,
    Rows,
}

/// The part of `old` that `new` replaces: where it starts, how many rows go and how many come.
/// Rows equal at both ends stay, so the list keeps their heights and the scroll stays put.
pub fn changed_span(old: &[Row], new: &[Row]) -> Option<(usize, usize, usize)> {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (gone, come) = (old.len() - head - tail, new.len() - head - tail);
    (gone > 0 || come > 0).then_some((head, gone, come))
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::git::LineKind;
    use artoo_core::rows::Cell;

    fn line(n: u32) -> Row {
        let cell = Cell {
            line: n,
            kind: LineKind::Context,
        };
        Row::Split {
            left: Some(cell),
            right: Some(cell),
        }
    }

    #[test]
    fn only_the_rows_that_differ_are_replaced() {
        let old = [
            line(1),
            Row::Gap {
                segment: 1,
                count: 9,
            },
            line(11),
        ];
        let new = [line(1), line(2), line(3), line(11)];
        assert_eq!(changed_span(&old, &new), Some((1, 1, 2)));
        assert_eq!(changed_span(&old, &old), None);
        // A thread put under a line.
        let new = [
            line(1),
            Row::Thread(4),
            Row::Gap {
                segment: 1,
                count: 9,
            },
            line(11),
        ];
        assert_eq!(changed_span(&old, &new), Some((1, 0, 1)));
    }

    #[test]
    fn a_row_is_found_in_its_block() {
        let mut stack = Stack::new(&[2, 0, 1], 3);
        stack.blocks[0].len = 4;
        stack.blocks[1].len = 0;
        stack.blocks[2].len = 2;
        stack.reindex();
        assert_eq!(stack.starts, vec![0, 5, 6, 9]);
        assert_eq!(stack.locate(0), (0, 0));
        assert_eq!(stack.locate(4), (0, 4));
        assert_eq!(stack.locate(5), (1, 0));
        assert_eq!(stack.locate(8), (2, 2));
        assert_eq!(stack.header(1), Some(6));
        assert_eq!(stack.block_of(2), Some(0));
    }
}
