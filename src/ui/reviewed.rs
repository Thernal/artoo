//! A reviewer's marks: which files of a diff were read. A ✓ per file, a count in the files label,
//! `v` to mark the open file and go on to the next one not yet read. Kept per commit, version
//! range or merge request on this Mac; a file that changes in a newer version loses its mark.
//! Designed in `../Design/mockups/ide-ideas/a-ideas.html` (1).

use super::px;
use super::{Header, ROW_RADIUS, Workspace, theme};
use artoo_review::reviewed::State;
use gpui::{App, ClickEvent, Context, IntoElement, div, prelude::*};

impl Workspace {
    /// Called when a diff arrives (the review indexed it, `Review::index`): a request's and a branch's pages
    /// learn how many files it has.
    pub(super) fn count_files(&mut self, cx: &mut App) {
        if let Some(d) = &self.review.read(cx).diff
            && let Header::Compare {
                request: Some(iid), ..
            } = d.header
        {
            let files = Some((iid, d.files.len()));
            self.requests.update(cx, |rq, _| rq.files = files);
        }
        if self
            .history
            .read(cx)
            .shows_branch(self.review.read(cx).diff.as_ref().map(|d| &d.header))
            && let Some(d) = self.review.read(cx).diff.as_ref()
        {
            let files = Some(d.files.len());
            self.history.update(cx, |h, _| {
                if let Some(f) = h.page.as_mut() {
                    f.files = files;
                }
            });
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

    /// The version of request `iid` your marks were last made at, by the versions loaded (#73).
    pub(super) fn marked_version(&self, iid: u64, cx: &App) -> Option<usize> {
        let root = self.root.as_ref()?.display().to_string();
        let heads = self
            .review
            .read(cx)
            .reviewed
            .marked_heads(&format!("{root}|mr:{iid}"));
        self.history
            .read(cx)
            .versions
            .iter()
            .filter(|v| heads.contains(&Some(v.tip)))
            .map(|v| v.number)
            .max()
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
        let ix = self.review.read(cx).file;
        let done = self.review.read(cx).review_state(ix) == Some(State::Done);
        self.set_reviewed(ix, !done, cx);
    }

    /// `v`: marks the open file read and opens the next one that is not.
    pub(super) fn mark_and_advance(&mut self, cx: &mut Context<Self>) {
        if self.review.read(cx).review_scope.is_none() {
            return;
        }
        self.set_reviewed(self.review.read(cx).file, true, cx);
        self.next_unreviewed(cx);
    }

    /// Opens the next file, in the tree's order, that is not marked read (`Review::next_unreviewed`).
    pub(super) fn next_unreviewed(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = self.review.update(cx, |rv, _| rv.next_unreviewed()) {
            self.select_file(ix, cx);
            self.review.read(cx).scroll_file_into_view(ix);
        }
    }

    /// The chip in the file header: mark it read, and the key.
    pub(super) fn render_review_chip(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let state = self
            .review
            .read(cx)
            .review_state(self.review.read(cx).file)?;
        Some(
            div()
                .id("review-chip")
                .flex_none()
                .h(px(24.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(ROW_RADIUS))
                .text_size(px(12.))
                .text_color(if state == State::Done {
                    theme::added()
                } else {
                    theme::muted()
                })
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child(match state {
                    State::Done => "✓ Reviewed",
                    State::Stale => "↻ Changed — mark again",
                    State::Open => "Mark reviewed",
                })
                .child(super::keys::caps(
                    if state == State::Done { "⇧V" } else { "V" },
                    super::keys::On::Plain,
                ))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    if this.review.read(cx).review_state(this.review.read(cx).file)
                        == Some(State::Done)
                    {
                        this.toggle_reviewed(cx)
                    } else {
                        this.mark_and_advance(cx)
                    }
                })),
        )
    }
}
