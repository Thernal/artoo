//! Annotate (⌥⌘B): beside each run of lines, who wrote it and when; a click opens the commit.
//! Read from `git blame` as of the revision on screen, in the background. Designed in
//! `../Design/mockups/ide-ideas/a-ideas.html` (2). A long hover on a line number says the same of
//! that one line, Annotate on or not (D19): each side at its own revision, read when first asked.

use super::{Workspace, format};
use gpui::Context;

impl Workspace {
    pub(super) fn toggle_annotate(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| {
            rv.annotate = !rv.annotate;
            rv.annotations.clear();
        });
        self.refresh_annotations(cx);
        cx.notify();
    }

    /// The annotation column of the open file read again, in the review's task
    /// (`Review::refresh_annotations`).
    pub(super) fn refresh_annotations(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, cx| rv.refresh_annotations(cx));
    }

    /// The pointer came on line number `line` of file `ix` (`at`), or went off it
    /// (`Review::hover_line_number`).
    pub(super) fn hover_line_number(
        &mut self,
        ix: usize,
        old: bool,
        line: u32,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut Context<Self>,
    ) {
        if self
            .review
            .update(cx, |rv, cx| rv.hover_line_number(ix, old, line, at, cx))
        {
            cx.notify();
        }
    }

    /// A click on an annotation: the commit that wrote line `line`, opened in the commits list.
    pub(super) fn open_annotated_commit(&mut self, line: u32, cx: &mut Context<Self>) {
        let Some(id) = line
            .checked_sub(1)
            .and_then(|i| self.review.read(cx).annotations.get(i as usize))
            .map(|c| c.id)
        else {
            return;
        };
        match self.commit_at(id, cx) {
            Some(ix) => {
                // The list may be filtered; show it as it is without the filter.
                self.pick(super::Selection::Commit(ix), cx);
            }
            None => {
                self.error = Some(
                    format!(
                        "{} is not in this list of commits (it is older, or on another branch).",
                        format::short(id)
                    )
                    .into(),
                );
                cx.notify();
            }
        }
    }
}
