//! The review's contract (#165 stage 4): what a page asks of the tab's review. For now the file Code reads
//! (#71, #75): read at a revision, which one is read, closed. The review draws it; the page puts it where it goes.
//!
//! The tab's review offers it as `Rc<dyn FileView>` in the tab's services (`Frame::services`). What it changes is
//! changed at once: ask it from anywhere but the review's own update.

use git2::Oid;
use gpui::App;
use std::path::PathBuf;

/// The file a page reads through the tab's review.
pub trait FileView {
    /// File `path` of project `root` read at `rev` (None: as it is on disk), in place of the one read.
    fn read(&self, root: PathBuf, path: String, rev: Option<Oid>, cx: &mut App);

    /// The path of the file read, when one is.
    fn path(&self, cx: &App) -> Option<String>;

    /// The file read is let go: nothing is read.
    fn close(&self, cx: &mut App);
}
