//! Going to a line (#7): `c` puts a marker on the diff, a line number typed moves it and ↵ opens the composer
//! there, on the lines selected at once; a run of unchanged lines that holds it opens. And a comment's place
//! shown: its file, its line in view, from the Agents window or the Inbox.

use super::input::{self, Edit};
use super::px;
use super::rows::Row;
use super::typing::{self, Input};
use super::{ROW_RADIUS, Workspace, theme};
use gpui::{App, Context, KeyDownEvent, WeakEntity, div, prelude::*};

impl Workspace {
    /// `c`: on the lines selected in the diff, the composer at once; else a marker appears on the
    /// diff, typing a line number moves it, ↵ opens the comment there.
    pub(super) fn open_goline(&mut self, cx: &mut Context<Self>) {
        // A comment left unfinished here: `c` goes back to it (#168).
        if self.compose.is_some() && self.give_back(Input::Compose, cx) {
            return cx.notify();
        }
        // On a folder's page in Code: a note for the agent on the folder (#97).
        if let Some(super::nav::Page::Folder(path)) = self.nav_page().cloned()
            && self.nav.space == super::spaces::Space::Code
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
            self.reset_field_edit();
            self.review
                .update(cx, |rv, _| rv.goline = Some(String::new()));
            self.take_keyboard(Input::GoLine, cx);
            return cx.notify();
        }
        if self.review.read(cx).data.is_none() || self.settings_open || self.compose.is_some() {
            return;
        }
        if let Some((old, first, last)) = self.review.read(cx).selected_lines() {
            self.zone = super::zones::Zone::Diff;
            return self.start_comment_on(old, Some(first), last, cx);
        }
        self.reset_field_edit();
        self.zone = super::zones::Zone::Diff;
        self.review
            .update(cx, |rv, _| rv.goline = Some(String::new()));
        self.take_keyboard(Input::GoLine, cx);
        self.move_marker(cx);
        cx.notify();
    }

    /// The line marker goes, nothing written.
    pub(super) fn close_goline(&mut self, cx: &mut App) {
        self.review.update(cx, |rv, _| {
            rv.goline = None;
            rv.goline_row = None;
        });
        self.refresh_rows(cx);
    }

    pub(super) fn goline_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(mut text) = self.review.read(cx).goline.clone() else {
            return;
        };
        let key = event.keystroke.key.as_str();
        if matches!(key, "up" | "down") {
            let step: i64 = if key == "up" { -1 } else { 1 };
            if let Some((old, line)) = self.review.read(cx).goline_target() {
                let next = (i64::from(line) + step).max(1);
                self.review.update(cx, |rv, _| {
                    rv.goline = Some(format!("{}{next}", if old { "-" } else { "" }))
                });
                self.move_marker(cx);
            }
            return cx.notify();
        }
        let edit = input::edit(&mut text, &mut self.field_edit, &event.keystroke, cx);
        self.review
            .update(cx, |rv, _| rv.goline = Some(text.clone()));
        match edit {
            Edit::Escape => self.close_goline(cx),
            // Digits, and a minus first for a removed line.
            Edit::Changed => {
                let keep = |(i, c): &(usize, char)| c.is_ascii_digit() || (*i == 0 && *c == '-');
                let filtered: String = text
                    .chars()
                    .enumerate()
                    .filter(keep)
                    .map(|(_, c)| c)
                    .collect();
                if filtered != text {
                    // The caret stays after the chars it followed.
                    let caret = self.field_edit.caret();
                    let before = text[..caret].chars().enumerate().filter(keep).count();
                    self.field_edit = crate::textedit::TextEdit::new(filtered.as_str());
                    self.field_edit.place(before, false);
                    self.review.update(cx, |rv, _| rv.goline = Some(filtered));
                }
                self.move_marker(cx);
            }
            Edit::Enter { .. } => return self.finish_goline(cx),
            Edit::Selected | Edit::Ignored => {}
        }
        cx.notify();
    }

    /// Puts the marker on the row of the typed line and scrolls it into view.
    fn move_marker(&mut self, cx: &mut App) {
        // The prompt row moves with the marker, so the rows are laid out again first.
        self.refresh_rows(cx);
        self.review.update(cx, |rv, _| {
            rv.goline_row = rv
                .goline_target()
                .and_then(|(old, line)| rv.row_of_line(line, old))
        });
        if let Some(at) = self.review.read(cx).goline_row {
            // The marked line and the prompt under it.
            self.review.update(cx, |rv, _| rv.reveal_rows(at..at + 2));
        } else if let Some(gap) = self
            .review
            .read(cx)
            .goline_target()
            .and_then(|(old, line)| self.review.read(cx).gap_of(line, old))
        {
            // Folded away: the run that holds it, which ↵ opens.
            self.review.update(cx, |rv, _| rv.reveal_rows(gap..gap + 1));
        }
    }

    /// Opens the composer under the marked line.
    fn finish_goline(&mut self, cx: &mut Context<Self>) {
        if self.code_file_shown(cx) {
            let line = self
                .review
                .read(cx)
                .goline
                .as_deref()
                .and_then(|t| t.parse::<u32>().ok())
                .unwrap_or(0);
            let count = self
                .review
                .read(cx)
                .file_view
                .as_ref()
                .map_or(0, |v| v.line_count());
            if line == 0 || line as usize > count {
                self.error = Some(format!("This file has {count} lines.").into());
                return cx.notify();
            }
            self.review.update(cx, |rv, _| rv.goline = None);
            return self.start_file_comment(line, cx);
        }
        let Some(data) = self.review.read(cx).data.clone() else {
            return;
        };
        let Some((old, line)) = self.review.read(cx).goline_target() else {
            return cx.notify();
        };
        if line == 0 || line > data.side(old).count() {
            self.error = Some(
                format!(
                    "This file has {} {} lines.",
                    data.side(old).count(),
                    if old { "old" } else { "new" }
                )
                .into(),
            );
            return cx.notify();
        }
        self.review.update(cx, |rv, _| {
            rv.goline = None;
            rv.goline_row = None;
        });
        self.reveal_line(line, old, cx);
        self.start_comment(old, line, cx);
        if let Some(at) = self.review.read(cx).row_of_line(line, old) {
            self.review.update(cx, |rv, _| rv.reveal_rows(at..at + 2));
        }
    }

    /// Opens the collapsed run of unchanged lines that holds `line`, if there is one.
    fn reveal_line(&mut self, line: u32, old: bool, cx: &mut App) {
        if self.review.read(cx).row_of_line(line, old).is_some() {
            return;
        }
        let Some(data) = &self.review.read(cx).data else {
            return;
        };
        let hit = data.segments.iter().position(|s| match s {
            super::rows::Segment::Same {
                old: o,
                new: n,
                len,
            } => {
                let start = if old { *o } else { *n };
                line >= start && line < start + len
            }
            super::rows::Segment::Change(_) => false,
        });
        if let Some(segment) = hit {
            self.review.update(cx, |rv, _| rv.expanded.insert(segment));
            self.relayout(cx);
        }
    }

    /// The prompt of `c` under the marked line.
    pub(super) fn render_goline_row(
        &self,
        indent: f32,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> gpui::AnyElement {
        let Some(text) = self.review.read(cx).goline.as_ref() else {
            return div().into_any_element();
        };
        let active = self.has_keyboard(Input::GoLine, cx);
        let row =
            div().w_full().pl(px(indent)).pr(px(12.)).py(px(4.)).child(
                div()
                    .max_w(px(640.))
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py(px(6.))
                    .rounded(px(ROW_RADIUS))
                    .border_2()
                    .border_color(theme::focus())
                    .bg(theme::panel())
                    .font_family(theme::ui_font())
                    .text_size(px(12.))
                    .child(div().text_color(theme::muted()).child("Comment on line"))
                    .child(div().min_w(px(56.)).font_family(theme::code_font()).child(
                        self.field_look(
                            text,
                            active,
                            "first change",
                            Default::default(),
                            this.clone(),
                        ),
                    ))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child("↵ write · ↑↓ step · −12 a removed line · esc"),
                    ),
            );
        typing::input_box_of(row, Input::GoLine, this).into_any_element()
    }

    pub(super) fn render_goline(
        &self,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> Option<impl IntoElement + use<>> {
        if self.review.read(cx).goline_row.is_some() {
            return None;
        }
        let text = self.review.read(cx).goline.as_ref()?;
        let target = self.review.read(cx).goline_target();
        let preview = target.and_then(|(old, line)| {
            let data = self.review.read(cx).data.as_ref()?;
            (line >= 1 && line <= data.side(old).count())
                .then(|| data.side(old).line(line).0.trim().to_owned())
        });
        let hint = match (target, &preview, self.review.read(cx).goline_row) {
            (None, ..) if !text.is_empty() => "type a line number".to_owned(),
            (None, ..) => "no changes in this file".to_owned(),
            (Some(_), None, _) => "no such line in this file".to_owned(),
            (Some(_), Some(_), None) => "inside unchanged lines · ↵ opens them".to_owned(),
            _ => "↵ comment here".to_owned(),
        };
        let active = self.has_keyboard(Input::GoLine, cx);
        let card = div()
            .absolute()
            .bottom(px(16.))
            .left(px(16.))
            .max_w(px(520.))
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .rounded(px(ROW_RADIUS))
            .border_2()
            .border_color(theme::focus())
            .bg(theme::panel())
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_color(theme::muted()).child("Comment on line"))
                    .child(div().min_w(px(60.)).font_family(theme::code_font()).child(
                        self.field_look(
                            text,
                            active,
                            "first change",
                            Default::default(),
                            this.clone(),
                        ),
                    ))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(hint),
                    ),
            )
            .children(preview.map(|code| {
                super::hover::cut("preview", code)
                    .font_family(theme::code_font())
                    .text_size(px(12.))
                    .text_color(theme::muted())
            }))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child("digits move the marker · ↑↓ step · −12 a removed line · ↵ open · esc"),
            );
        Some(typing::input_box_of(card, Input::GoLine, this))
    }

    /// Shows a comment: in the open file, scrolled to; in another file of the diff, that file first.
    pub(super) fn jump_to(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some((path, line, old)) = self
            .comments
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
        if self.review.read(cx).current_path().as_ref() == Some(&path) {
            self.apply_jump(cx);
            return;
        }
        let ix = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .and_then(|d| d.files.iter().position(|f| f.path() == path));
        match ix {
            Some(ix) => self.select_file(ix, cx),
            // Not in the diff open: the file in Code at its line, the note drawn under it there
            // (the developer, 2026-10-23: a note asking you led nowhere).
            None if !old && self.root.is_some() => {
                self.jump = None;
                self.file_jump = Some((path.clone(), line));
                self.nav.space = super::spaces::Space::Code;
                self.open_code_file(path, false, cx);
            }
            None => self.jump = None,
        }
    }

    /// Scrolls to the line and the comments under it that a jump asked for, once its file is laid
    /// out: the line, opened first when it is folded away, with the discussions or the pending
    /// comment below it in view.
    pub(super) fn apply_jump(&mut self, cx: &mut Context<Self>) {
        if let Some((line, old)) = self.review.update(cx, |rv, _| rv.jump_line.take()) {
            self.reveal_line(line, old, cx);
            if self
                .review
                .update(cx, |rv, _| std::mem::take(&mut rv.jump_whole))
            {
                self.reveal_function(line, old, cx);
            }
            if let Some(at) = self.review.read(cx).row_of_line(line, old) {
                let under = self.review.read(cx).rows[at + 1..]
                    .iter()
                    .take_while(|r| {
                        matches!(
                            r,
                            Row::Request(_) | Row::Draft(_) | Row::Thread(_) | Row::Note(_)
                        )
                    })
                    .count();
                self.review
                    .update(cx, |rv, _| rv.reveal_rows(at..at + 1 + under));
                cx.notify();
            }
        }
        let Some(id) = self.jump.take() else {
            return;
        };
        if let Some(at) = self
            .review
            .read(cx)
            .rows
            .iter()
            .position(|r| *r == Row::Thread(id))
        {
            self.review
                .update(cx, |rv, _| rv.reveal_rows(at.saturating_sub(1)..at + 1));
            cx.notify();
        }
    }
}
