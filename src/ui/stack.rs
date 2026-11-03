//! Every file of a diff in one scroll (Design `docs/specs/all-files.md`): the diff's list holds a
//! block per file, a header row and the file's rows under it.
//!
//! One file is *active* — the one in view, or the one last clicked. Its data, rows, opened runs and
//! comment marks are `Review::data`, `rows`, `expanded` and `notes`, numbered from its first
//! row, so everything that works on one file (find, F7, `c`, selections, annotate, bookmarks)
//! works on it unchanged; `list_base` turns its row numbers into the list's. The other blocks keep
//! theirs in the stack (`artoo_review::stack`) and are drawn as they are until a click or the scroll
//! makes them active.

use super::Workspace;
use artoo_review::reviewed::State;
use gpui::{App, Context};

impl Workspace {
    /// Every file in one scroll: the option is on and the diff has more than one file.
    pub(super) fn wants_stack(&self, cx: &App) -> bool {
        !self.view_options(cx).one_file
            && !(self.zen_one && self.zen.is_some())
            && self
                .review
                .read(cx)
                .diff
                .as_ref()
                .is_some_and(|d| d.files.len() > 1)
    }

    /// Puts the files of the diff in one list and shows file `ix` at the top (`Review::open_stack`), then
    /// builds them.
    fn open_stack(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.review.update(cx, |rv, _| rv.open_stack(ix)) {
            self.build_stack(cx);
        }
    }

    /// Builds the stack again (a new diff mode), keeping the place.
    pub(super) fn reopen_stack(&mut self, cx: &mut Context<Self>) {
        if self.review.update(cx, |rv, _| rv.reopen_stack()) {
            self.build_stack(cx);
        }
    }

    /// Picking file `ix` (the files island, ← →, `v`, a link): it becomes active, opens when it
    /// was folded, and its header comes to the top.
    pub(super) fn stack_select(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.review.read(cx).stack.is_none() {
            return self.open_stack(ix, cx);
        }
        self.activate_file(ix, cx);
        if self.review.read(cx).folded(ix) {
            self.flip_fold(ix, cx);
        }
        self.review.update(cx, |rv, _| rv.scroll_to_header(ix));
        if self.diff_data(cx).is_some() {
            self.apply_jump(cx);
        }
        self.build_stack(cx);
        cx.notify();
    }

    /// Makes file `ix` the one the keyboard and the header above the diff work on
    /// (`Review::activate_file`).
    pub(super) fn activate_file(&mut self, ix: usize, cx: &mut Context<Self>) {
        let pins = self.open_pins(cx);
        if self.review.update(cx, |rv, _| rv.activate_file(ix, &pins)) {
            self.refresh_annotations(cx);
            cx.notify();
        }
    }

    /// Folds file `ix` to its header, or opens it.
    pub(super) fn flip_fold(&mut self, ix: usize, cx: &mut Context<Self>) {
        // Its rows fade in as it opens (#56, 9).
        self.mark_motion(&format!("file-{ix}"), cx);
        if self.review.update(cx, |rv, _| rv.flip_fold(ix)) {
            self.build_stack(cx);
            cx.notify();
        }
    }

    /// View → Fold reviewed files changed: every file goes back to its default.
    pub(super) fn refold(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| rv.refold());
        self.build_stack(cx);
    }

    /// The scroll moved the active file out of view: the file at the top takes over, and the files
    /// scrolled past are marked reviewed when Settings says so (`Review::scrolled_to`).
    pub(super) fn follow_scroll(&mut self, cx: &mut Context<Self>) {
        // The file being written in stays while the diff scrolls; one left does not (#168).
        if self.has_keyboard(super::typing::Input::Compose, cx) {
            return;
        }
        let Some((to, passed)) = self.review.read(cx).scrolled_to() else {
            return;
        };
        self.activate_file(to, cx);
        let passed: Vec<usize> = passed
            .into_iter()
            .filter(|&ix| self.review.read(cx).review_state(ix) == Some(State::Open))
            .collect();
        if !passed.is_empty() {
            self.set_reviewed_many(&passed, true, cx);
        }
    }

    // ---- building -----------------------------------------------------------------------------

    /// Builds the files' rows in the review's task (`Review::build_stack`).
    pub(super) fn build_stack(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, cx| rv.build_stack(cx));
    }

    /// File `ix`'s rows were built (`ReviewEvent::StackBuilt`): laid out with what is pinned on its lines
    /// (`Review::built`); the active file's blame and jump follow.
    pub(super) fn stack_built(&mut self, ix: usize, cx: &mut Context<Self>) {
        let path = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .and_then(|d| d.files.get(ix))
            .map(|f| f.path().to_owned());
        let pins = self.pins_for(path, cx);
        if self.review.update(cx, |rv, _| rv.built(ix, &pins)) {
            self.refresh_annotations(cx);
            self.apply_jump(cx);
        }
    }

    /// Lays every file out again (`Review::relayout_stack`) with what is pinned on their lines now.
    pub(super) fn relayout_stack(&mut self, cx: &mut App) {
        let pins = self.all_pins(cx);
        self.review.update(cx, |rv, _| rv.relayout_stack(&pins));
    }

    // ---- folding all, finding in all ----------------------------------------------------------

    /// ⌥⇧A: every file open when some are folded, else every file folded.
    pub(super) fn toggle_expand_all(&mut self, cx: &mut Context<Self>) {
        let open = self.review.read(cx).any_folded();
        self.expand_all(open, cx);
    }

    /// Opens every file (`open`), or folds every file to its header.
    pub(super) fn expand_all(&mut self, open: bool, cx: &mut Context<Self>) {
        self.mark_motion("files", cx);
        if self.review.update(cx, |rv, _| rv.expand_all(open)) {
            self.build_stack(cx);
            cx.notify();
        }
    }

    // ---- drawing ------------------------------------------------------------------------------

    /// Shows file `ix` alone (an image, which has no rows), until all files are asked for again.
    pub(super) fn view_alone(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.set_option_unsaved(super::Opt::OneFile, true, cx);
        self.select_file(ix, cx);
    }
}
