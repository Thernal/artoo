//! Every file of a diff in one scroll (Design `docs/specs/all-files.md`): the diff's list holds a
//! block per file, a header row and the file's rows under it.
//!
//! One file is *active* — the one in view, or the one last clicked. Its data, rows, opened runs and
//! comment marks are `Review::data`, `rows`, `expanded` and `notes`, numbered from its first
//! row, so everything that works on one file (find, F7, `c`, selections, annotate, bookmarks)
//! works on it unchanged; `list_base` turns its row numbers into the list's. The other blocks keep
//! theirs in the stack (`artoo_review::stack`) and are drawn as they are until a click or the scroll
//! makes them active.

use super::pins::Pins;
use super::review::{FileEvent, Host, ReviewEvent};
use super::rows::{self, FileData, Row};
use super::{ROW_RADIUS, Workspace, px, theme};
use crate::git::FileDiff;
use artoo_review::reviewed::State;
use artoo_review::stack::{Body, CARD_GAP, CARD_MARGIN, CARD_RADIUS, Stack, changed_span};
use gpui::{
    App, ClickEvent, Context, FontWeight, ListOffset, MouseButton, WeakEntity, div, prelude::*,
};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

impl Workspace {
    /// Every file in one scroll: the option is on and the diff has more than one file.
    pub(super) fn wants_stack(&self, cx: &App) -> bool {
        !self.review.read(cx).options.one_file
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
        if self.review.read(cx).data.is_some() {
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

    /// The match `at` of every file's: its file becomes active and opens, the row comes into view.
    pub(super) fn go_to_stack_match(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some((file, row)) = self.review.update(cx, |rv, _| rv.stack_match(at)) else {
            return;
        };
        self.activate_file(file, cx);
        self.review.update(cx, |rv, _| rv.scroll_to_row(row));
        cx.notify();
    }

    // ---- drawing ------------------------------------------------------------------------------

    /// Shows file `ix` alone (an image, which has no rows), until all files are asked for again.
    pub(super) fn view_alone(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.set_option_unsaved(super::Opt::OneFile, true, cx);
        self.select_file(ix, cx);
    }
}

impl super::review::Review {
    /// A file's header in the list, the top of the file's card: a click folds or opens it, the
    /// chip at the right marks it reviewed. Cards stand apart on the darker ground between them.
    pub(super) fn render_header(
        &self,
        ix: usize,
        this: WeakEntity<Self>,
        host: &Rc<dyn Host>,
        cx: &App,
    ) -> gpui::AnyElement {
        let Some(file) = self.diff.as_ref().and_then(|d| d.files.get(ix)) else {
            return div().into_any_element();
        };
        let path = match (&file.old_path, &file.new_path) {
            (Some(old), Some(new)) if old != new => format!("{old} → {new}"),
            _ => file.path().to_owned(),
        };
        let folded = self.folded(ix);
        let state = self.review_state(ix);
        let active = ix == self.file;
        let first = self.stack.as_ref().and_then(|s| s.block_of(ix)) == Some(0);
        let (fold, mark) = (host.clone(), this);
        div()
            .id(("file-header", ix))
            .w_full()
            .pt(px(if first { 4. } else { CARD_GAP }))
            .px(px(CARD_MARGIN))
            .child(
                div()
                    .w_full()
                    .h(px(40.))
                    .pl(px(6.))
                    .pr(px(8.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded_t(px(CARD_RADIUS))
                    .when(folded, |s| s.rounded_b(px(CARD_RADIUS)))
                    .border_1()
                    .when(!folded, |s| s.border_b_0())
                    .border_color(if active {
                        theme::focus()
                    } else {
                        theme::island_border()
                    })
                    .bg(theme::panel())
                    .font_family(theme::ui_font())
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(
                        div()
                            .size(px(24.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            // The chevron turns as the card folds or opens (#56, 9).
                            .child(super::motion::toward(
                                super::icons::icon("chevron")
                                    .size(px(16.))
                                    .text_color(theme::muted()),
                                "card-chevron",
                                !folded,
                                false,
                                super::motion::FAST,
                                |chevron, v| {
                                    chevron.with_transformation(gpui::Transformation::rotate(
                                        gpui::radians(-std::f32::consts::FRAC_PI_2 * (1. - v)),
                                    ))
                                },
                            )),
                    )
                    .child(super::icons::file_icon(file.path(), 16.))
                    .child(
                        super::hover::cut(("file-path", ix), path)
                            .flex_1()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(super::change_color(file, state == Some(State::Done))),
                    )
                    // The working tree's part it is (#22): staged, not staged, untracked.
                    .children(super::working::file_tag(
                        self.diff.as_ref().and_then(|d| d.tags.word(ix)),
                    ))
                    // Another chat is changing it (#77).
                    .children(host.badge(file.path(), cx))
                    .children(host.page_switch(ix, cx))
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .gap_1()
                            .text_size(px(12.))
                            .child(
                                div()
                                    .text_color(theme::added())
                                    .child(format!("+{}", file.added)),
                            )
                            .child(
                                div()
                                    .text_color(theme::removed())
                                    .child(format!("−{}", file.removed)),
                            ),
                    )
                    .children(state.map(|state| {
                        div()
                            .id(("file-header-mark", ix))
                            .flex_none()
                            .px_2()
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .rounded(px(ROW_RADIUS))
                            .text_size(px(12.))
                            .text_color(match state {
                                State::Done => theme::added(),
                                State::Stale => theme::warning(),
                                State::Open => theme::muted(),
                            })
                            .font_weight(if state == State::Done {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .hover(|s| s.bg(theme::selected()).text_color(theme::text()))
                            .child(match state {
                                State::Done => "✓ Reviewed",
                                State::Stale => "↻ Changed — mark again",
                                State::Open => "Mark reviewed",
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(move |_: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                mark.update(cx, |rv, cx| {
                                    let done = rv.review_state(ix) == Some(State::Done);
                                    if rv.set_reviewed(ix, !done) {
                                        cx.emit(ReviewEvent::Changed);
                                    }
                                })
                                .ok();
                            })
                    })),
            )
            // A double click folds or opens it once, as one click does.
            .on_click(move |e: &ClickEvent, _, cx| {
                if e.click_count() > 1 {
                    return;
                }
                fold.file(FileEvent::Fold(ix), cx);
            })
            .into_any_element()
    }

    /// The one line under a header that has no rows to show (or none yet).
    pub(super) fn render_note(
        &self,
        b: usize,
        ix: usize,
        this: WeakEntity<Self>,
        host: &Rc<dyn Host>,
        cx: &gpui::App,
    ) -> gpui::AnyElement {
        let Some(file) = self.diff.as_ref().and_then(|d| d.files.get(ix)) else {
            return div().into_any_element();
        };
        let line = |text: String| {
            div()
                .w_full()
                .px(px(18.))
                .py(px(8.))
                .font_family(theme::ui_font())
                .text_size(px(12.))
                .text_color(theme::muted())
                .flex()
                .items_center()
                .gap_3()
                .child(text)
        };
        let action = |id: &'static str, label: &'static str| {
            div()
                .id((id, ix))
                .px_2()
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .bg(theme::hover())
                .text_color(theme::text())
                .cursor_pointer()
                .hover(|s| s.bg(theme::selected()))
                .child(label)
        };
        if self.body(b) == Body::Waiting {
            let rows = (file.added + file.removed + 6).min(14) as f32;
            // Its rows are being built: a shimmer as tall as they will about be (#56, 18).
            return line(String::new())
                .h(px(self.row_height() * rows))
                .items_start()
                .child(super::motion::shimmer(
                    ("card-loading", ix),
                    (rows as usize / 2).max(2),
                ))
                .into_any_element();
        }
        if self.generated_folded(file) {
            let path = file.path().to_owned();
            return line(format!(
                "Generated: {} changed lines are folded.",
                file.added + file.removed
            ))
            .child(action("unfold-in-stack", "Show it anyway").on_click(
                move |_: &ClickEvent, _, cx| {
                    this.update(cx, |rv, cx| {
                        rv.unfold(&path, cx);
                        cx.emit(ReviewEvent::Changed);
                    })
                    .ok();
                },
            ))
            .into_any_element();
        }
        if self.shows_page(file) {
            return self.card_row(host.page(ix, cx), ix, true);
        }
        if self.shows_image(file) {
            let alone = host.clone();
            return line("An image.".to_owned())
                .child(action("image-alone", "View it on its own").on_click(
                    move |_: &ClickEvent, _, cx| {
                        alone.file(FileEvent::Alone(ix), cx);
                    },
                ))
                .into_any_element();
        }
        let text = if file.hunks.is_empty() {
            file.note
                .clone()
                .unwrap_or_else(|| "No content changes".to_owned())
        } else {
            "No changes".to_owned()
        };
        line(text).into_any_element()
    }

    /// The list index of the active file's first row.
    pub(super) fn list_base(&self) -> usize {
        self.stack
            .as_ref()
            .and_then(|s| s.header(self.file))
            .map_or(0, |h| h + 1)
    }

    /// The active file's row at the top of the view (its first while the view is above it).
    pub(super) fn local_top(&self) -> usize {
        self.diff_list
            .logical_scroll_top()
            .item_ix
            .saturating_sub(self.list_base())
    }

    /// The place the view is at: the file at the top and the rows below its header (the open
    /// file and the list's offset when files are shown one at a time).
    pub(super) fn scroll_place(&self) -> Option<(String, ListOffset)> {
        let diff = self.diff.as_ref()?;
        let top = self.diff_list.logical_scroll_top();
        let Some(stack) = &self.stack else {
            return Some((diff.files.get(self.file)?.path().to_owned(), top));
        };
        let (b, inside) = stack.locate(top.item_ix);
        let file = diff.files.get(stack.blocks.get(b)?.file)?;
        Some((
            file.path().to_owned(),
            // Counted from the file's first row, as when files are shown one at a time.
            ListOffset {
                item_ix: inside.saturating_sub(1),
                offset_in_item: top.offset_in_item,
            },
        ))
    }

    fn scroll_to_header(&mut self, ix: usize) {
        if let Some(at) = self.stack.as_ref().and_then(|s| s.header(ix)) {
            self.diff_list.scroll_to(ListOffset {
                item_ix: at,
                offset_in_item: px(0.),
            });
        }
    }

    /// The widest line of the files built, for the horizontal scroll.
    pub(super) fn widest(&self) -> Option<usize> {
        let mine = self.data.as_ref().map(|d| d.widest);
        let others = self
            .stack
            .iter()
            .flat_map(|s| &s.blocks)
            .filter_map(|b| b.data.as_ref().map(|d| d.widest));
        mine.into_iter().chain(others).max()
    }

    /// The find text's rows in every file built, folded ones too.
    pub(super) fn recompute_stack_matches(&mut self) {
        let Some(stack) = &self.stack else {
            return;
        };
        let mut matches = Vec::new();
        if !self.dterms.is_empty() {
            for block in &stack.blocks {
                let (data, rows) = if block.file == self.file {
                    (self.data.as_deref(), &self.rows[..])
                } else {
                    (block.data.as_deref(), &block.rows[..])
                };
                if let Some(data) = data {
                    matches.extend(
                        rows::matching_rows(data, rows, &self.dterms)
                            .into_iter()
                            .map(|row| (block.file, row)),
                    );
                }
            }
        }
        if let Some(stack) = self.stack.as_mut() {
            stack.match_at = stack.match_at.min(matches.len().saturating_sub(1));
            stack.matches = matches;
        }
    }

    /// The find bar's count with all files in one scroll: "2 of 9 lines · 3 files".
    pub(super) fn stack_match_count(&self) -> Option<String> {
        let stack = self.stack.as_ref()?;
        if stack.matches.is_empty() {
            return Some("No matches".to_owned());
        }
        let files = stack
            .matches
            .iter()
            .map(|(file, _)| file)
            .collect::<HashSet<_>>()
            .len();
        Some(format!(
            "{} of {} lines · {}",
            stack.match_at + 1,
            stack.matches.len(),
            super::plural(files, "file")
        ))
    }

    /// Where a new find starts: the first match in the file in view or after it.
    pub(super) fn first_stack_match(&self) -> Option<usize> {
        let stack = self.stack.as_ref()?;
        let here = stack.block_of(self.file)?;
        let top = self.local_top();
        stack
            .matches
            .iter()
            .position(|&(file, row)| {
                let b = stack.block_of(file).unwrap_or(0);
                b > here || (b == here && row >= top)
            })
            .or((!stack.matches.is_empty()).then_some(0))
    }

    /// A row of a file's card: its sides framed, and the last one closes the card.
    pub(super) fn card_row(
        &self,
        row: gpui::AnyElement,
        file: usize,
        last: bool,
    ) -> gpui::AnyElement {
        let border = if file == self.file {
            theme::focus()
        } else {
            theme::island_border()
        };
        div()
            .w_full()
            .px(px(CARD_MARGIN))
            .child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .bg(theme::editor())
                    .border_l_1()
                    .border_r_1()
                    .border_color(border)
                    .when(last, |s| {
                        s.border_b_1().rounded_b(px(CARD_RADIUS)).pb(px(4.))
                    })
                    .child(row),
            )
            .into_any_element()
    }

    fn needs_rows(&self, file: &FileDiff) -> bool {
        !file.hunks.is_empty()
            && !self.generated_folded(file)
            && !self.shows_image(file)
            && !self.shows_page(file)
    }

    /// File `ix` folds by default: it is reviewed and View → Fold reviewed files is on.
    fn folds_by_default(&self, ix: usize) -> bool {
        self.options.fold_reviewed && self.review_state(ix) == Some(State::Done)
    }

    /// File `ix` shows its header alone: by default, unless the reader opened it (or the other
    /// way round).
    pub(super) fn folded(&self, ix: usize) -> bool {
        self.folds_by_default(ix) != self.stack.as_ref().is_some_and(|s| s.flipped.contains(&ix))
    }

    pub(super) fn body(&self, b: usize) -> Body {
        let (Some(stack), Some(diff)) = (&self.stack, &self.diff) else {
            return Body::Note;
        };
        let block = &stack.blocks[b];
        let Some(file) = diff.files.get(block.file) else {
            return Body::Note;
        };
        let (built, rows) = if block.file == self.file {
            (self.data.is_some(), self.rows.len())
        } else {
            (block.data.is_some(), block.rows.len())
        };
        if self.folded(block.file) {
            Body::Folded
        } else if !self.needs_rows(file) {
            Body::Note
        } else if !built {
            Body::Waiting
        } else if rows == 0 {
            Body::Note
        } else {
            Body::Rows
        }
    }

    fn body_len(&self, b: usize) -> usize {
        match self.body(b) {
            Body::Folded => 0,
            Body::Waiting | Body::Note => 1,
            Body::Rows
                if self
                    .stack
                    .as_ref()
                    .is_some_and(|s| s.blocks[b].file == self.file) =>
            {
                self.rows.len()
            }
            Body::Rows => self.stack.as_ref().map_or(0, |s| s.blocks[b].rows.len()),
        }
    }

    /// Block `b`'s rows as the list holds them, when it holds rows.
    pub(super) fn shown_rows(&self, b: usize) -> Option<Vec<Row>> {
        let stack = self.stack.as_ref()?;
        (self.body(b) == Body::Rows).then(|| {
            if stack.blocks[b].file == self.file {
                self.rows.clone()
            } else {
                stack.blocks[b].rows.clone()
            }
        })
    }

    /// Tells the list that block `b` changed from `before` (its rows, when it showed rows): only
    /// the rows that differ are replaced.
    pub(super) fn stack_changed(&mut self, b: usize, before: Option<Vec<Row>>) {
        let after = self.shown_rows(b);
        let len = self.body_len(b);
        let Some(stack) = self.stack.as_mut() else {
            return;
        };
        let at = stack.starts[b] + 1;
        match (before, after) {
            (Some(old), Some(new)) => {
                if let Some((head, gone, come)) = changed_span(&old, &new) {
                    self.diff_list.splice(at + head..at + head + gone, come);
                }
            }
            _ => {
                let old = stack.blocks[b].len;
                self.diff_list.splice(at..at + old, len);
            }
        }
        stack.blocks[b].len = len;
        stack.reindex();
    }

    /// Brings every block's length in the list in line with what it shows now (after files were
    /// marked reviewed, folded or opened).
    pub(super) fn sync_stack(&mut self) {
        let Some(n) = self.stack.as_ref().map(|s| s.blocks.len()) else {
            return;
        };
        let lens: Vec<usize> = (0..n).map(|b| self.body_len(b)).collect();
        let Some(stack) = self.stack.as_mut() else {
            return;
        };
        // From the last block up, so the list indexes of the blocks above stay right.
        for b in (0..n).rev() {
            let old = stack.blocks[b].len;
            if old != lens[b] {
                let at = stack.starts[b] + 1;
                self.diff_list.splice(at..at + old, lens[b]);
                stack.blocks[b].len = lens[b];
            }
        }
        stack.reindex();
    }

    /// Files marked or unmarked reviewed fold or open as their mark says again. Only with Fold
    /// reviewed files on does the mark say anything: else a file the reader folded stays so (#144).
    pub(super) fn review_changed(&mut self, ixs: &[usize]) {
        if !self.options.fold_reviewed {
            return;
        }
        let Some(stack) = self.stack.as_mut() else {
            return;
        };
        for ix in ixs {
            stack.flipped.remove(ix);
        }
        self.sync_stack();
    }

    fn next_to_build(&self) -> Option<(usize, FileDiff, crate::storage::DiffMode)> {
        // A find looks into folded files too.
        let finding = !self.dterms.is_empty();
        let (stack, diff) = (self.stack.as_ref()?, self.diff.as_ref()?);
        let here = stack.block_of(self.file).unwrap_or(0);
        let n = stack.blocks.len();
        let near =
            (0..n).flat_map(|d| [here.checked_add(d), here.checked_sub(d).filter(|_| d > 0)]);
        near.flatten().filter(|&b| b < n).find_map(|b| {
            let ix = stack.blocks[b].file;
            let built = if ix == self.file {
                self.data.is_some()
            } else {
                stack.blocks[b].data.is_some()
            };
            let file = diff.files.get(ix)?;
            (!built && (finding || !self.folded(ix)) && self.needs_rows(file))
                .then(|| (ix, file.clone(), self.options.mode))
        })
    }

    /// A document drawn as a page is one item of the list: measured again after it changed
    /// (Split or Unified, a section opened).
    pub(super) fn remeasure_pages(&mut self) {
        let (Some(stack), Some(diff)) = (&self.stack, &self.diff) else {
            return;
        };
        let at: Vec<usize> = stack
            .blocks
            .iter()
            .enumerate()
            .filter(|(b, block)| {
                diff.files
                    .get(block.file)
                    .is_some_and(|f| self.shows_page(f))
                    && self.body(*b) == Body::Note
            })
            .map(|(b, _)| stack.starts[b] + 1)
            .collect();
        for at in at {
            self.diff_list.splice(at..at + 1, 1);
        }
    }

    /// Some file shows its header alone.
    pub(super) fn any_folded(&self) -> bool {
        self.stack
            .as_ref()
            .is_some_and(|s| s.blocks.iter().any(|b| self.folded(b.file)))
    }

    /// Opens the active file when it is folded: something in it is about to be shown.
    pub(super) fn open_active(&mut self) {
        if self.stack.is_some() && self.folded(self.file) {
            if let Some(stack) = self.stack.as_mut()
                && !stack.flipped.remove(&self.file)
            {
                stack.flipped.insert(self.file);
            }
            self.sync_stack();
        }
    }

    /// Lays block `b` (not the active one) out with `data`, `pins` under their lines.
    pub(super) fn lay_block(&mut self, b: usize, path: &str, data: Arc<FileData>, pins: &Pins) {
        let Some(stack) = self.stack.as_ref() else {
            return;
        };
        let rows = rows::layout(&data, &self.view_options(), &stack.blocks[b].expanded);
        let rows = self.with_pins_in(path, &data, rows, false, pins);
        let notes = self.line_notes_in(path, Some(&data), pins);
        if let Some(block) = self.stack.as_mut().map(|s| &mut s.blocks[b]) {
            block.data = Some(data);
            block.rows = rows;
            block.notes = notes;
        }
    }

    /// Lays every file out again (an option changed, a comment came or went) with `pins`, changing in
    /// the list only the rows that differ.
    pub(super) fn relayout_stack(&mut self, pins: &Pins) {
        let Some(n) = self.stack.as_ref().map(|s| s.blocks.len()) else {
            return;
        };
        for b in 0..n {
            let Some((ix, data)) = self
                .stack
                .as_ref()
                .map(|s| (s.blocks[b].file, s.blocks[b].data.clone()))
            else {
                return;
            };
            if ix == self.file {
                self.relayout(pins);
                continue;
            }
            let (Some(data), Some(path)) = (
                data,
                self.diff
                    .as_ref()
                    .and_then(|d| d.files.get(ix))
                    .map(|f| f.path().to_owned()),
            ) else {
                continue;
            };
            let before = self.shown_rows(b);
            self.lay_block(b, &path, data, pins);
            self.stack_changed(b, before);
        }
        self.remeasure_pages();
        // Files that now fold, or no longer do (Settings → Fold generated files).
        self.sync_stack();
        self.recompute_stack_matches();
    }

    /// Puts the files of the diff in one list and shows file `ix` at the top; false without a diff.
    pub(super) fn open_stack(&mut self, ix: usize) -> bool {
        let Some(diff) = &self.diff else {
            return false;
        };
        let order: Vec<usize> = super::lists::file_items(
            &diff.files,
            self.options.list_files,
            "",
            &HashSet::new(),
            diff.tags.staged,
        )
        .into_iter()
        .filter_map(|item| match item {
            super::lists::FileItem::File { ix, .. } => Some(ix),
            _ => None,
        })
        .collect();
        let files = diff.files.len();
        // The place a reload keeps: rows below the header of the file it was in.
        let anchor = self
            .restore_scroll
            .take()
            .filter(|(path, _)| diff.files.get(ix).is_some_and(|f| f.path() == path))
            .map(|(_, offset)| offset);
        self.clear_file();
        self.file = ix;
        let mut stack = Stack::new(&order, files);
        stack.anchor = anchor;
        self.stack = Some(stack);
        let n = order.len();
        let lens: Vec<usize> = (0..n).map(|b| self.body_len(b)).collect();
        if let Some(stack) = self.stack.as_mut() {
            for (block, len) in stack.blocks.iter_mut().zip(lens) {
                block.len = len;
            }
            stack.reindex();
            self.diff_list.reset(stack.total());
        }
        self.scroll_to_header(ix);
        true
    }

    /// Puts the files in one list again (a new diff mode), keeping the place; false without a diff.
    pub(super) fn reopen_stack(&mut self) -> bool {
        self.restore_scroll = self.scroll_place();
        let ix = self
            .restore_scroll
            .as_ref()
            .and_then(|(path, _)| {
                self.diff
                    .as_ref()?
                    .files
                    .iter()
                    .position(|f| f.path() == path)
            })
            .unwrap_or(self.file);
        self.open_stack(ix)
    }

    /// Makes file `ix` the one the keyboard and the header above the diff work on, `pins` laying out
    /// the file it leaves when the `c` prompt goes; false when it already is, or is not in the stack.
    pub(super) fn activate_file(&mut self, ix: usize, pins: &Pins) -> bool {
        if self.file == ix || self.stack.is_none() {
            return false;
        }
        // The prompt of `c` belongs to the file it was opened in.
        if self.goline.take().is_some() {
            self.goline_row = None;
            self.relayout(pins);
        }
        let Some(stack) = self.stack.as_mut() else {
            return false;
        };
        let (Some(from), Some(to)) = (stack.block_of(self.file), stack.block_of(ix)) else {
            return false;
        };
        let block = &mut stack.blocks[from];
        block.data = self.data.take();
        block.rows = std::mem::take(&mut self.rows);
        block.expanded = std::mem::take(&mut self.expanded);
        block.notes = std::mem::take(&mut self.notes);
        let block = &mut stack.blocks[to];
        self.data = block.data.take();
        self.rows = std::mem::take(&mut block.rows);
        self.expanded = std::mem::take(&mut block.expanded);
        self.notes = std::mem::take(&mut block.notes);
        self.file = ix;
        self.fcursor = None;
        self.sel = None;
        self.change_at = None;
        self.match_at = 0;
        self.recompute_matches();
        self.scroll_file_into_view(ix);
        true
    }

    /// Folds file `ix` to its header, or opens it; false without a stack.
    pub(super) fn flip_fold(&mut self, ix: usize) -> bool {
        let Some(stack) = self.stack.as_mut() else {
            return false;
        };
        if !stack.flipped.remove(&ix) {
            stack.flipped.insert(ix);
        }
        self.sync_stack();
        true
    }

    /// Every file back to its default fold.
    pub(super) fn refold(&mut self) {
        if let Some(stack) = self.stack.as_mut() {
            stack.flipped.clear();
        }
        self.sync_stack();
    }

    /// When the scroll moved the active file out of view: the file at the top, which takes over, and the
    /// files scrolled past downwards to mark reviewed (Settings → Diff → *Mark reviewed as I scroll
    /// past*). Nothing while the `c` prompt is open or text is being selected.
    pub(super) fn scrolled_to(&self) -> Option<(usize, Vec<usize>)> {
        if self.goline.is_some() || self.selecting {
            return None;
        }
        let stack = self.stack.as_ref()?;
        let b = stack.block_of(self.file)?;
        let span = stack.span(b);
        let above = self.diff_list.item_is_above_viewport(span.end - 1) == Some(true);
        let below = !above && self.diff_list.item_is_below_viewport(span.start) == Some(true);
        if !above && !below {
            return None;
        }
        let (top, _) = stack.locate(self.diff_list.logical_scroll_top().item_ix);
        let to = stack.blocks[top].file;
        if to == self.file {
            return None;
        }
        // Every file from the one left to the one now at the top was scrolled past, also those a
        // page skipped over.
        let passed: Vec<usize> = if above && self.settings.auto_reviewed {
            stack.blocks[b..top]
                .iter()
                .map(|block| block.file)
                .collect()
        } else {
            Vec::new()
        };
        Some((to, passed))
    }

    /// Builds the files' rows in the background, one at a time: the active file first, then the
    /// files nearest to it. Each is kept and said when built (`ReviewEvent::StackBuilt`).
    pub(super) fn build_stack(&mut self, cx: &mut Context<Self>) {
        let Some(stack) = self.stack.as_mut().filter(|s| !s.building) else {
            return;
        };
        stack.building = true;
        stack.task = Some(cx.spawn(async move |this, cx| {
            loop {
                let next = this
                    .update(cx, |rv, _| {
                        let next = rv.next_to_build();
                        if next.is_none()
                            && let Some(stack) = rv.stack.as_mut()
                        {
                            stack.building = false;
                        }
                        next
                    })
                    .ok()
                    .flatten();
                let Some((ix, file, mode)) = next else {
                    break;
                };
                let data = cx
                    .background_executor()
                    .spawn(async move { FileData::build(&file, mode) })
                    .await;
                if this
                    .update(cx, |rv, cx| {
                        rv.keep_built(ix, data);
                        cx.emit(ReviewEvent::StackBuilt(ix));
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    /// File `ix`'s rows were built as `data`: kept until they are laid out (`Review::built`).
    fn keep_built(&mut self, ix: usize, data: FileData) {
        let data = Some(Arc::new(data));
        if ix == self.file {
            self.data = data;
        } else if let Some(stack) = self.stack.as_mut()
            && let Some(b) = stack.block_of(ix)
        {
            stack.blocks[b].data = data;
        }
    }

    /// File `ix`'s rows, built and kept, are laid out with `pins`, the active file scrolled to the
    /// place a reload kept. True when it is the active file.
    pub(super) fn built(&mut self, ix: usize, pins: &Pins) -> bool {
        if ix == self.file {
            self.relayout(pins);
            if let Some(anchor) = self.stack.as_mut().and_then(|s| s.anchor.take())
                && let Some(header) = self.stack.as_ref().and_then(|s| s.header(ix))
            {
                self.diff_list.scroll_to(ListOffset {
                    item_ix: header + 1 + anchor.item_ix,
                    ..anchor
                });
            }
            return true;
        }
        if let Some(b) = self.stack.as_ref().and_then(|s| s.block_of(ix)) {
            let (Some(path), Some(data)) = (
                self.diff
                    .as_ref()
                    .and_then(|d| d.files.get(ix))
                    .map(|f| f.path().to_owned()),
                self.stack.as_ref().and_then(|s| s.blocks[b].data.clone()),
            ) else {
                return false;
            };
            let before = self.shown_rows(b);
            self.lay_block(b, &path, data, pins);
            self.stack_changed(b, before);
            self.recompute_stack_matches();
        }
        false
    }

    /// Opens every file (`open`), or folds every file to its header; false without a stack.
    pub(super) fn expand_all(&mut self, open: bool) -> bool {
        let Some(files) = self
            .stack
            .as_ref()
            .map(|s| s.blocks.iter().map(|b| b.file).collect::<Vec<_>>())
        else {
            return false;
        };
        // The files whose default is the other way round.
        let flipped: HashSet<usize> = files
            .into_iter()
            .filter(|&ix| self.folds_by_default(ix) == open)
            .collect();
        if let Some(stack) = self.stack.as_mut() {
            stack.flipped = flipped;
        }
        self.sync_stack();
        true
    }

    /// Match `at` of every file's becomes the current one: its file and row.
    pub(super) fn stack_match(&mut self, at: usize) -> Option<(usize, usize)> {
        let &(file, row) = self.stack.as_ref()?.matches.get(at)?;
        if let Some(stack) = self.stack.as_mut() {
            stack.match_at = at;
        }
        Some((file, row))
    }
}
