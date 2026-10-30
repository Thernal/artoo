//! What other features put on a file's lines, as data the review lays out (#165 stage 4): discussions with
//! their replies, pending comments, the composer, bookmarks, and the agents' comments and notes, placed against
//! the file's text when it is laid out. The tab gathers them (`Workspace::pins_of`); the review asks no one while
//! it works.

use super::Workspace;
use super::diff_view::{LineNote, NoteTone};
use super::review::Review;
use super::rows::{self, FileData, Pin, Row};
use crate::review::{self, Comment, Place};
use gpui::App;
use std::collections::HashMap;

/// What other features put on one file's lines.
#[derive(Default)]
pub(super) struct FilePins {
    /// The agents' notes on the new side, in the order drawn (#151): id, the line written on, its text then, and
    /// the line followed to the diff shown when the text no longer matches (#46).
    pub notes: Vec<(u64, u32, String, Option<u32>)>,
    /// The agents' comments, with the line followed when their text no longer matches (#46).
    pub comments: Vec<(Comment, Option<u32>)>,
    /// Rows under lines known without the text: discussions each with its replies, pending comments, the
    /// composer.
    pub fixed: Vec<Pin>,
    /// Gutter marks known without the text: bookmarks, discussions, pending comments.
    pub marks: Vec<(bool, u32, NoteTone)>,
    /// A composer is open in this file; the `c` prompt does not hang beside it.
    pub composing: bool,
}

impl FilePins {
    /// Where an agent's comment is in the file built as `data`.
    fn comment_line(data: &FileData, (comment, followed): &(Comment, Option<u32>)) -> Option<u32> {
        let side = data.side(comment.old);
        match review::place(comment, side.count(), |n| side.line(n).0) {
            Place::Line(n) => Some(n),
            Place::Outdated => *followed,
        }
    }

    /// The pins of the file built as `data`, the `c` prompt's composer under `go` unless one is open here.
    pub fn rows(&self, data: &FileData, go: Option<(bool, u32)>) -> Vec<Pin> {
        let side = data.side(false);
        // Why the line is as it is comes before what is said about it.
        let notes = self.notes.iter().filter_map(|(id, line, code, followed)| {
            let at = match review::find_line(*line, code, side.count(), |l| side.line(l).0) {
                Place::Line(l) => Some(l),
                Place::Outdated => *followed,
            };
            at.map(|line| Pin {
                old: false,
                line,
                row: Row::Note(*id),
            })
        });
        let comments = self.comments.iter().filter_map(|c| {
            Some(Pin {
                old: c.0.old,
                line: Self::comment_line(data, c)?,
                row: Row::Thread(c.0.id),
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
                if let Some(n) = Self::comment_line(data, c) {
                    add(c.0.old, n, NoteTone::Agent);
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
pub(super) type Pins = HashMap<String, FilePins>;

/// The tab's side: what is pinned on the files a layout lays out, gathered now.
impl Workspace {
    /// The pins of the open file.
    pub(super) fn open_pins(&self, cx: &App) -> Pins {
        self.pins_for(self.review.read(cx).current_path(), cx)
    }

    /// The pins of every file of the diff.
    pub(super) fn all_pins(&self, cx: &App) -> Pins {
        let paths: Vec<String> = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .map(|d| d.files.iter().map(|f| f.path().to_owned()).collect())
            .unwrap_or_default();
        self.pins_for(paths, cx)
    }

    /// The pins of the files at `paths`.
    pub(super) fn pins_for(&self, paths: impl IntoIterator<Item = String>, cx: &App) -> Pins {
        paths
            .into_iter()
            .map(|path| {
                let pins = self.pins_of(&path, cx);
                (path, pins)
            })
            .collect()
    }
}

/// The review's side: the rows and marks of a file with what others pinned on it.
impl Review {
    /// The open file's rows with what is pinned on its lines.
    pub(super) fn with_pins(&self, rows: Vec<Row>, pins: &Pins) -> Vec<Row> {
        match (self.current_path(), &self.data) {
            (Some(path), Some(data)) => self.with_pins_in(&path, data, rows, true, pins),
            _ => rows,
        }
    }

    /// The rows of file `path` with what is pinned on its lines; the `c` prompt only in the `active` file, the one
    /// the keyboard works in.
    pub(super) fn with_pins_in(
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
    pub(super) fn line_notes(&self, pins: &Pins) -> Vec<LineNote> {
        self.current_path()
            .map(|path| self.line_notes_in(&path, self.data.as_deref(), pins))
            .unwrap_or_default()
    }

    /// The comment marks of file `path`'s gutter, `data` its built diff.
    pub(super) fn line_notes_in(
        &self,
        path: &str,
        data: Option<&FileData>,
        pins: &Pins,
    ) -> Vec<LineNote> {
        pins.get(path).map(|p| p.marks(data)).unwrap_or_default()
    }
}
