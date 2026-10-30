//! What other features put on a file's lines, as data the review lays out (#165 stage 4): discussions with
//! their replies, pending comments, the composer, bookmarks, and the agents' comments and notes, placed against
//! the file's text when it is laid out. The tab gathers them (`Workspace::pins_of`); the review asks no one while
//! it works.

use super::Workspace;
use artoo_review::pins::Pins;
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
