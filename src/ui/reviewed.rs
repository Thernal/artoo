//! A reviewer's marks: which files of a diff were read. A ✓ per file, a count in the files label,
//! `v` to mark the open file and go on to the next one not yet read. Kept per commit, version
//! range or merge request on this Mac; a file that changes in a newer version loses its mark.
//! Designed in `../Design/mockups/ide-ideas/a-ideas.html` (1).

use super::{Header, Workspace};
use artoo_review_api::ReviewState as State;
use gpui::{App, Context};

impl Workspace {
    /// Called when a diff arrives (the review indexed it, `Review::index`): a request's and a branch's pages
    /// learn how many files it has.
    pub(super) fn count_files(&mut self, cx: &mut App) {
        if let Some(d) = &self.shown_diff(cx)
            && let Header::Compare {
                request: Some(iid), ..
            } = d.header
        {
            let files = Some((iid, d.files.len()));
            if let Some(requests) = self.requests_api() {
                requests.set_files(files, cx);
            }
        }
        if let Some(branches) = self.branches() {
            branches.count_files(cx);
        }
    }

    pub(super) fn set_reviewed(&mut self, ix: usize, done: bool, cx: &mut Context<Self>) {
        if self.review.update(cx, |rv, _| rv.set_reviewed(ix, done)) {
            cx.notify();
        }
    }

    /// Marks or unmarks files `ixs` at once (all of them, a folder).
    pub(super) fn set_reviewed_many(&mut self, ixs: &[usize], done: bool, cx: &mut Context<Self>) {
        if self
            .review
            .update(cx, |rv, _| rv.set_reviewed_many(ixs, done))
        {
            cx.notify();
        }
    }

    /// ⌥⇧V and the buttons: every file the list shows marked, or (when all are) unmarked.
    pub(super) fn toggle_all_reviewed(&mut self, cx: &mut Context<Self>) {
        let shown = self.review.read(cx).file_order();
        let done = shown
            .iter()
            .all(|&ix| self.review.read(cx).review_state(ix) == Some(State::Done));
        self.set_reviewed_many(&shown, !done, cx);
    }

    /// `⇧V`: marks or unmarks the open file.
    pub(super) fn toggle_reviewed(&mut self, cx: &mut Context<Self>) {
        let ix = self.diff_file(cx);
        let done = self.review.read(cx).review_state(ix) == Some(State::Done);
        self.set_reviewed(ix, !done, cx);
    }

    /// `v`: marks the open file read and opens the next one that is not.
    pub(super) fn mark_and_advance(&mut self, cx: &mut Context<Self>) {
        if self.review.read(cx).review_scope.is_none() {
            return;
        }
        self.set_reviewed(self.diff_file(cx), true, cx);
        self.next_unreviewed(cx);
    }

    /// Opens the next file, in the tree's order, that is not marked read (`Review::next_unreviewed`).
    pub(super) fn next_unreviewed(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = self.review.update(cx, |rv, _| rv.next_unreviewed()) {
            self.select_file(ix, cx);
            self.review.read(cx).scroll_file_into_view(ix);
        }
    }
}
