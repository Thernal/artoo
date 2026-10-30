//! What other features put on a file's lines, as data the review lays out (#165 stage 4): discussions with
//! their replies, pending comments, the composer, bookmarks, and the agents' comments and notes, placed against
//! the file's text when it is laid out. The tab gathers them (`Workspace::pins_of`); the review asks no one while
//! it works.

use super::Workspace;
use super::diff_view::LineNote;
use super::review::Review;
use super::rows::{self, FileData, Row};
use artoo_review::pins::{FilePins, Pins};
use gpui::App;

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
