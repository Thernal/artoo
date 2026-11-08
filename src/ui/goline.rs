//! Going to a line (#7), the tab's side: `c` opens the composer on the lines selected at once, on a folder's page
//! or a file in Code its note, else the review's marker (`artoo_review::goline`), whose line is one of the review's
//! inputs; what it did comes back here, where the rows are laid out and the composer opens. And a comment's place
//! shown: its file, its line in view, from the Agents window or the Inbox.

use super::Workspace;
use super::rows::Row;
use super::typing::{self, Input};
use artoo_review_api::GoLineKey;
use gpui::{App, Context, WeakEntity, div, prelude::*};

impl Workspace {
    /// `c`: on the lines selected in the diff, the composer at once; else a marker appears on the
    /// diff, typing a line number moves it, ↵ opens the comment there.
    pub(super) fn open_goline(&mut self, cx: &mut Context<Self>) {
        // A comment left unfinished here: `c` goes back to it (#168).
        if self.compose.is_some() && self.give_back(Input::Compose, cx) {
            return cx.notify();
        }
        // On a folder's page in Code: a note for the agent on the folder (#97).
        if let Some(super::nav::Page::Folder(path)) = self.shown_page()
            && self.nav.space == super::spaces::CODE
            && !self.settings_open
            && self.compose.is_none()
        {
            return self.start_folder_comment(&path, cx);
        }
        // On a file read in Code with lines selected: a note on them (#104).
        if self.code_file_shown(cx)
            && !self.settings_open
            && self.compose.is_none()
            && let Some((first, last)) = self.file_selected_lines(cx)
        {
            self.note_sel = None;
            return self.start_file_comment_on(Some(first), last, cx);
        }
        // On a file read in Code: the number of the line to write a note on (#75).
        if self.code_file_shown(cx) && !self.settings_open && self.compose.is_none() {
            self.rv().open_goline(cx);
            self.take_keyboard(Input::Part(artoo_review_api::GOLINE_INPUT), cx);
            return cx.notify();
        }
        if self.diff_data(cx).is_none() || self.settings_open || self.compose.is_some() {
            return;
        }
        if let Some((old, first, last)) = self.rv().selected_lines(cx) {
            self.zone = super::zones::Zone::Diff;
            return self.start_comment_on(old, Some(first), last, cx);
        }
        self.zone = super::zones::Zone::Diff;
        self.rv().open_goline(cx);
        self.take_keyboard(Input::Part(artoo_review_api::GOLINE_INPUT), cx);
        self.move_marker(cx);
        cx.notify();
    }

    /// The line marker goes, nothing written.
    pub(super) fn close_goline(&mut self, cx: &mut App) {
        self.rv().close_goline(cx);
        self.refresh_rows(cx);
    }

    /// What the line typed after `c` did (`ReviewEvent::GoLine`): the marker moved, the composer opens on its
    /// line, or it closed; the rows laid out again with what is pinned on them.
    pub(super) fn goline_done(&mut self, done: GoLineKey, cx: &mut Context<Self>) {
        match done {
            GoLineKey::Moved => self.move_marker(cx),
            GoLineKey::Finish => self.finish_goline(cx),
            GoLineKey::Close => self.refresh_rows(cx),
            GoLineKey::Kept => {}
        }
    }

    /// Puts the marker on the row of the typed line and scrolls it into view.
    fn move_marker(&mut self, cx: &mut App) {
        // The prompt row moves with the marker, so the rows are laid out again first.
        self.refresh_rows(cx);
        self.rv().place_marker(cx);
    }

    /// Opens the composer under the marked line.
    fn finish_goline(&mut self, cx: &mut Context<Self>) {
        let file = self.code_file_shown(cx);
        match self.rv().goline_at(file, cx) {
            None => cx.notify(),
            Some(Err(why)) => {
                self.error = Some(why.into());
                cx.notify();
            }
            Some(Ok((_, line))) if file => self.start_file_comment(line, cx),
            Some(Ok((old, line))) => {
                self.reveal_line(line, old, cx);
                self.start_comment(old, line, cx);
                if let Some(at) = self.rv().row_of_line(line, old, cx) {
                    self.rv().reveal_rows(at..at + 2, cx);
                }
            }
        }
    }

    /// Opens the collapsed run of unchanged lines that holds `line`, if there is one.
    fn reveal_line(&mut self, line: u32, old: bool, cx: &mut App) {
        if self.rv().open_run_of(line, old, cx) {
            self.relayout(cx);
        }
    }

    /// The prompt of `c` under the marked line, the review's (`Review::render_goline_row`).
    pub(super) fn render_goline_row(
        &self,
        indent: f32,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> gpui::AnyElement {
        let input = Input::Part(artoo_review_api::GOLINE_INPUT);
        let active = self.has_keyboard(input, cx);
        match self.rv().render_goline_row(indent, active, cx) {
            Some(row) => typing::input_box_of(row, input, this).into_any_element(),
            None => div().into_any_element(),
        }
    }

    /// Shows a comment: in the open file, scrolled to; in another file of the diff, that file first.
    pub(super) fn jump_to(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some((path, line, old)) = self
            .comments(cx)
            .iter()
            .find(|c| c.id == id)
            .map(|c| (c.path.clone(), c.line, c.old))
        else {
            return;
        };
        // A folder's note: the folder's page, where it is written (#97).
        if let Some(folder) = path.strip_suffix('/') {
            return self.open_folder_page(folder.to_owned(), cx);
        }
        self.jump = Some(id);
        if self.current_path(cx).as_ref() == Some(&path) {
            self.apply_jump(cx);
            return;
        }
        let ix = self
            .shown_diff(cx)
            .as_ref()
            .and_then(|d| d.files.iter().position(|f| f.path() == path));
        match ix {
            Some(ix) => self.select_file(ix, cx),
            // Not in the diff open: the file in Code at its line, the note drawn under it there
            // (the developer, 2026-10-23: a note asking you led nowhere).
            None if !old && self.root.is_some() => {
                self.jump = None;
                self.file_jump = Some((path.clone(), line));
                self.nav.space = super::spaces::CODE;
                self.open_code_file(path, false, cx);
            }
            None => self.jump = None,
        }
    }

    /// Scrolls to the line and the comments under it that a jump asked for, once its file is laid
    /// out: the line, opened first when it is folded away, with the discussions or the pending
    /// comment below it in view.
    pub(super) fn apply_jump(&mut self, cx: &mut Context<Self>) {
        if let Some((line, old)) = self.rv().take_jump(cx) {
            self.reveal_line(line, old, cx);
            if self.rv().take_jump_whole(cx) {
                self.reveal_function(line, old, cx);
            }
            if let Some(at) = self.rv().row_of_line(line, old, cx) {
                let under = self.rv().rows(cx)[at + 1..]
                    .iter()
                    .take_while(|r| {
                        matches!(
                            r,
                            Row::Request(_)
                                | Row::Draft(_)
                                | Row::Thread(_)
                                | Row::Note(_)
                                | Row::Decoration(_)
                        )
                    })
                    .count();
                self.rv().reveal_rows(at..at + 1 + under, cx);
                cx.notify();
            }
        }
        let Some(id) = self.jump.take() else {
            return;
        };
        if let Some(at) = (self.rv().rows(cx).iter()).position(|r| *r == Row::Thread(id)) {
            self.rv().reveal_rows(at.saturating_sub(1)..at + 1, cx);
            cx.notify();
        }
    }
}
