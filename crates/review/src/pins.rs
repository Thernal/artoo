//! What other features put on a file's lines, as data the review lays out (#165 stage 4): discussions with their
//! replies, pending comments, the composer, bookmarks, and the agents' comments and notes, placed against the file's
//! text when it is laid out.

use crate::diff_view::{LineNote, NoteTone};
use artoo_core::anchor::{Place, find_line};
use artoo_core::rows::{FileData, Pin, Row};
use std::collections::HashMap;

/// A line an agent wrote on: what it is (a comment's or a note's id), the side, the line written on and its text then,
/// and the line followed to the diff shown when the text no longer matches (#46).
pub struct Anchored {
    pub id: u64,
    pub old: bool,
    pub line: u32,
    pub code: String,
    pub followed: Option<u32>,
}

impl Anchored {
    /// Where it is in the file built as `data`.
    fn line_in(&self, data: &FileData) -> Option<u32> {
        let side = data.side(self.old);
        match find_line(self.line, &self.code, side.count(), |n| side.line(n).0) {
            Place::Line(n) => Some(n),
            Place::Outdated => self.followed,
        }
    }
}

/// What other features put on one file's lines.
#[derive(Default)]
pub struct FilePins {
    /// The agents' notes on the new side, in the order drawn (#151).
    pub notes: Vec<Anchored>,
    /// The agents' comments.
    pub comments: Vec<Anchored>,
    /// Rows under lines known without the text: discussions each with its replies, pending comments, the
    /// composer.
    pub fixed: Vec<Pin>,
    /// Gutter marks known without the text: bookmarks, discussions, pending comments.
    pub marks: Vec<(bool, u32, NoteTone)>,
    /// A composer is open in this file; the `c` prompt does not hang beside it.
    pub composing: bool,
}

impl FilePins {
    /// The pins of the file built as `data`, the `c` prompt's composer under `go` unless one is open here.
    pub fn rows(&self, data: &FileData, go: Option<(bool, u32)>) -> Vec<Pin> {
        // Why the line is as it is comes before what is said about it.
        let notes = self.notes.iter().filter_map(|n| {
            Some(Pin {
                old: false,
                line: n.line_in(data)?,
                row: Row::Note(n.id),
            })
        });
        let comments = self.comments.iter().filter_map(|c| {
            Some(Pin {
                old: c.old,
                line: c.line_in(data)?,
                row: Row::Thread(c.id),
            })
        });
        let go = go.filter(|_| !self.composing).map(|(old, line)| Pin {
            old,
            line,
            row: Row::Composer,
        });
        notes
            .chain(comments)
            .chain(self.fixed.iter().copied())
            .chain(go)
            .collect()
    }

    /// The gutter's marks of the file built as `data`: one per line with comments, in the colour of the most
    /// important kind there (the request's, then one's pending, then the agent's).
    pub fn marks(&self, data: Option<&FileData>) -> Vec<LineNote> {
        let mut out: Vec<LineNote> = Vec::new();
        let mut add = |old: bool, line: u32, tone: NoteTone| match out
            .iter_mut()
            .find(|n| n.old == old && n.line == line)
        {
            Some(n) => {
                n.count += 1;
                let rank = |t: NoteTone| match t {
                    NoteTone::Request => 0,
                    NoteTone::Draft => 1,
                    NoteTone::Agent => 2,
                    NoteTone::Bookmark => 3,
                };
                if rank(tone) < rank(n.tone) {
                    n.tone = tone;
                }
            }
            None => out.push(LineNote {
                old,
                line,
                tone,
                count: 1,
            }),
        };
        if let Some(data) = data {
            for c in &self.comments {
                if let Some(n) = c.line_in(data) {
                    add(c.old, n, NoteTone::Agent);
                }
            }
        }
        for &(old, line, tone) in &self.marks {
            add(old, line, tone);
        }
        out
    }
}

/// What others put on the lines of each file, by path: gathered by the tab for the files a layout lays out.
pub type Pins = HashMap<String, FilePins>;
