//! Going to a line (#7): the line typed after `c` — digits, a minus first for a removed line, ↑ ↓ stepping it —,
//! the marker on its row, the run of unchanged lines that holds it opened, where ↵ writes, and the prompt drawn:
//! under the marked line, or over the diff while the line is on no row. The tab keeps which input has the keys and
//! draws the field typed in.

use crate::review::Review;
use artoo_core::rows::Segment;
use artoo_core::textedit::TextEdit;
use artoo_ui_kit::input::{self, Edit};
use artoo_ui_kit::{ROW_RADIUS, hover, px, theme};
use gpui::{AnyElement, App, Div, Keystroke, div, prelude::*};

/// What a key did to the line typed after `c`.
pub enum GoLineKey {
    /// Esc: the marker goes.
    Close,
    /// The line changed: the marker moves once the rows are laid out again (`place_marker`).
    Moved,
    /// ↵: the comment opens there (`goline_at`).
    Finish,
    /// Nothing for the marker.
    Kept,
}

impl Review {
    /// The line after `c` starts empty: it stands for the first change.
    pub fn open_goline(&mut self) {
        self.goline = Some(String::new());
    }

    /// The marker goes, nothing written.
    pub fn close_goline(&mut self) {
        self.goline = None;
        self.goline_row = None;
    }

    /// Key `key` on the line typed, its caret and selection kept in `edit`.
    pub fn goline_key(&mut self, key: &Keystroke, edit: &mut TextEdit, cx: &mut App) -> GoLineKey {
        let Some(mut text) = self.goline.clone() else {
            return GoLineKey::Kept;
        };
        if matches!(key.key.as_str(), "up" | "down") {
            let step: i64 = if key.key == "up" { -1 } else { 1 };
            let Some((old, line)) = self.goline_target() else {
                return GoLineKey::Kept;
            };
            let next = (i64::from(line) + step).max(1);
            self.goline = Some(format!("{}{next}", if old { "-" } else { "" }));
            return GoLineKey::Moved;
        }
        let done = input::edit(&mut text, edit, key, cx);
        self.goline = Some(text.clone());
        match done {
            Edit::Escape => GoLineKey::Close,
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
                    let before = text[..edit.caret()]
                        .chars()
                        .enumerate()
                        .filter(keep)
                        .count();
                    *edit = TextEdit::new(filtered.as_str());
                    edit.place(before, false);
                    self.goline = Some(filtered);
                }
                GoLineKey::Moved
            }
            Edit::Enter { .. } => GoLineKey::Finish,
            Edit::Selected | Edit::Ignored => GoLineKey::Kept,
        }
    }

    /// The marker on the row of the typed line, it and the prompt under it in view; folded away, the run that
    /// holds it, which ↵ opens. The rows are laid out first: the prompt's row moves with the marker.
    pub fn place_marker(&mut self) {
        self.goline_row = self
            .goline_target()
            .and_then(|(old, line)| self.row_of_line(line, old));
        if let Some(at) = self.goline_row {
            self.reveal_rows(at..at + 2);
        } else if let Some(gap) = self
            .goline_target()
            .and_then(|(old, line)| self.gap_of(line, old))
        {
            self.reveal_rows(gap..gap + 1);
        }
    }

    /// Where ↵ on the line typed writes — a line of the file read in Code (`file`), else a side's line of the
    /// diff's file —, the marker gone; why not when there is no such line; None with no file laid out.
    pub fn goline_at(&mut self, file: bool) -> Option<Result<(bool, u32), String>> {
        if file {
            let line = self
                .goline
                .as_deref()
                .and_then(|t| t.parse::<u32>().ok())
                .unwrap_or(0);
            let count = self.file_view.as_ref().map_or(0, |v| v.line_count());
            if line == 0 || line as usize > count {
                return Some(Err(format!("This file has {count} lines.")));
            }
            self.goline = None;
            return Some(Ok((false, line)));
        }
        let data = self.data.clone()?;
        let (old, line) = self.goline_target()?;
        let count = data.side(old).count();
        if line == 0 || line > count {
            let side = if old { "old" } else { "new" };
            return Some(Err(format!("This file has {count} {side} lines.")));
        }
        self.close_goline();
        Some(Ok((old, line)))
    }

    /// The collapsed run of unchanged lines that holds `line` opened, unless the line shows: whether there was one
    /// (the rows are laid out again then).
    pub fn open_run_of(&mut self, line: u32, old: bool) -> bool {
        if self.row_of_line(line, old).is_some() {
            return false;
        }
        let Some(data) = &self.data else {
            return false;
        };
        let hit = data.segments.iter().position(|s| match s {
            Segment::Same {
                old: o,
                new: n,
                len,
            } => {
                let start = if old { *o } else { *n };
                line >= start && line < start + len
            }
            Segment::Change(_) => false,
        });
        hit.map(|segment| self.expanded.insert(segment)).is_some()
    }

    /// The prompt of `c` under the marked line, `indent` in; `field` the line typed, as the tab draws it.
    pub fn render_goline_row(&self, indent: f32, field: AnyElement) -> Div {
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
                .child(
                    div()
                        .min_w(px(56.))
                        .font_family(theme::code_font())
                        .child(field),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child("↵ write · ↑↓ step · −12 a removed line · esc"),
                ),
        )
    }

    /// The prompt of `c` over the diff while the line is on no row: what it aims at and the line's text; None
    /// while the marker is on a row or nothing is typed. `field` the line typed, as the tab draws it.
    pub fn render_goline(&self, field: AnyElement) -> Option<Div> {
        if self.goline_row.is_some() {
            return None;
        }
        let text = self.goline.as_ref()?;
        let target = self.goline_target();
        let preview = target.and_then(|(old, line)| {
            let data = self.data.as_ref()?;
            (line >= 1 && line <= data.side(old).count())
                .then(|| data.side(old).line(line).0.trim().to_owned())
        });
        let hint = match (target, &preview, self.goline_row) {
            (None, ..) if !text.is_empty() => "type a line number".to_owned(),
            (None, ..) => "no changes in this file".to_owned(),
            (Some(_), None, _) => "no such line in this file".to_owned(),
            (Some(_), Some(_), None) => "inside unchanged lines · ↵ opens them".to_owned(),
            _ => "↵ comment here".to_owned(),
        };
        Some(
            div()
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
                        .child(
                            div()
                                .min_w(px(60.))
                                .font_family(theme::code_font())
                                .child(field),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(hint),
                        ),
                )
                .children(preview.map(|code| {
                    hover::cut("preview", code)
                        .font_family(theme::code_font())
                        .text_size(px(12.))
                        .text_color(theme::muted())
                }))
                .child(
                    div().text_size(px(11.)).text_color(theme::faint()).child(
                        "digits move the marker · ↑↓ step · −12 a removed line · ↵ open · esc",
                    ),
                ),
        )
    }
}
