//! What other features put on a file's lines, as data the review lays out (#165 stage 4): discussions with their
//! replies, pending comments, the composer, bookmarks, and the agents' comments and notes, placed against the file's
//! text when it is laid out.

use crate::diff_view::{LineNote, NoteTone};
use crate::review::Review;
use artoo_core::anchor::{Place, find_line};
use artoo_core::rows::{self, FileData, Pin, Row};

pub use artoo_review_api::{Anchored, FilePins, Pins};

/// Where an anchored comment is on the file now.
trait Placed {
    fn line_in(&self, data: &FileData) -> Option<u32>;
}

impl Placed for Anchored {
    /// Where it is in the file built as `data`.
    fn line_in(&self, data: &FileData) -> Option<u32> {
        let side = data.side(self.old);
        match find_line(self.line, &self.code, side.count(), |n| side.line(n).0) {
            Place::Line(n) => Some(n),
            Place::Outdated => self.followed,
        }
    }
}

/// What a file's pins come to: its rows, its gutter's marks.
pub trait Laid {
    fn rows(&self, data: &FileData, go: Option<(bool, u32)>) -> Vec<Pin>;
    fn marks(&self, data: Option<&FileData>) -> Vec<LineNote>;
}

impl Laid for FilePins {
    /// The pins of the file built as `data`, the `c` prompt's composer under `go` unless one is open here.
    fn rows(&self, data: &FileData, go: Option<(bool, u32)>) -> Vec<Pin> {
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
    fn marks(&self, data: Option<&FileData>) -> Vec<LineNote> {
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
                    NoteTone::Marked(_) => 4,
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

/// The review's side: the rows and marks of a file with what others pinned on it.
impl Review {
    /// The open file's rows with what is pinned on its lines.
    pub fn with_pins(&self, rows: Vec<Row>, pins: &Pins) -> Vec<Row> {
        match (self.current_path(), &self.data) {
            (Some(path), Some(data)) => self.with_pins_in(&path, data, rows, true, pins),
            _ => rows,
        }
    }

    /// The rows of file `path` with what is pinned on its lines; the `c` prompt only in the `active` file, the one
    /// the keyboard works in.
    pub fn with_pins_in(
        &self,
        path: &str,
        data: &FileData,
        rows: Vec<Row>,
        active: bool,
        pins: &Pins,
    ) -> Vec<Row> {
        let none = FilePins::default();
        let pins = pins.get(path).unwrap_or(&none);
        // The prompt of `c` hangs under the marked line, like the composer that follows it.
        let go = if active && !pins.composing && self.goline.is_some() {
            self.goline_target()
        } else {
            None
        };
        rows::pinned(rows, &pins.rows(data, go))
    }

    /// The comment marks of the open file's gutter.
    pub fn line_notes(&self, pins: &Pins) -> Vec<LineNote> {
        self.current_path()
            .map(|path| self.line_notes_in(&path, self.data.as_deref(), pins))
            .unwrap_or_default()
    }

    /// The comment marks of file `path`'s gutter, `data` its built diff.
    pub fn line_notes_in(&self, path: &str, data: Option<&FileData>, pins: &Pins) -> Vec<LineNote> {
        pins.get(path).map(|p| p.marks(data)).unwrap_or_default()
    }
}
