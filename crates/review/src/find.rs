//! The find over the diff (⌘F): the rows that hold the text and each change's start, stepping through them in
//! the open file or in every file of the one scroll, the rows brought into view; its bar, and its field as one of
//! the tab's inputs (`input`).

use crate::review::{Review, ReviewEvent};
use artoo_core::rows::Row;
use artoo_core::{rows, search};
use artoo_shell_api::{Input, Typed};
use artoo_ui_kit::controls::chip;
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::input::{self, Area, Edit};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme};
use gpui::{App, ClickEvent, Context, Div, Entity, WeakEntity, div, prelude::*};
use std::rc::Rc;
use std::sync::Arc;

/// Where a find goes in the one scroll: a file and its row.
/// The find's field among the tab's inputs.
pub const INPUT: &str = artoo_review_api::FIND_INPUT;

pub type Found = Option<(usize, usize)>;

impl Review {
    /// The find bar above the diff: its field (`active` with the keyboard), "2 of 4 lines", previous, next,
    /// close; `this` answers its clicks.
    pub fn render_find_bar(&self, active: bool, this: WeakEntity<Review>) -> Option<Div> {
        let text = self.dfind.as_ref()?;
        let field = self.find_area.field_of(
            text,
            active,
            "Find in this diff",
            false,
            |rv: &mut Review| Some(&mut rv.find_area),
            this.clone(),
        );
        let count = if let Some(error) = &self.dfind_error {
            format!("Not a regular expression: {error}")
        } else if self.dterms.is_empty() {
            String::new()
        } else if let Some(count) = self.stack_match_count() {
            count
        } else if self.matches.is_empty() {
            "No matches".to_owned()
        } else {
            format!("{} of {} lines", self.match_at + 1, self.matches.len())
        };
        let click = |act: fn(&mut Review, &mut Context<Review>)| {
            let this = this.clone();
            move |_: &ClickEvent, _: &mut gpui::Window, cx: &mut App| {
                this.update(cx, act).ok();
            }
        };
        let step = |id: &'static str, label: &'static str, act| {
            chip(id, label, false).on_click(click(act))
        };
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px(px(10.))
                .py(px(6.))
                .border_b_1()
                .border_color(theme::island_border())
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .h(px(26.))
                        .px_2()
                        .rounded(px(ROW_RADIUS))
                        .border_2()
                        .border_color(if active {
                            theme::focus()
                        } else {
                            theme::island_border()
                        })
                        .bg(theme::base())
                        .cursor_text()
                        .child(
                            icons::icon("search")
                                .size(px(14.))
                                .text_color(theme::faint()),
                        )
                        .child(field)
                        .child(
                            chip("dfind-regex", ".*", self.dfind_regex)
                                .tip("Regular expression  ⌥R")
                                .on_click(click(|rv, cx| {
                                    let found = rv.toggle_find_regex(cx);
                                    rv.found(found, cx)
                                })),
                        ),
                )
                .child(
                    hover::cut("find-count", count.clone())
                        .text_size(px(12.))
                        .text_color(if count == "No matches" || self.dfind_error.is_some() {
                            theme::removed()
                        } else {
                            theme::muted()
                        }),
                )
                .child(step("dfind-prev", "↑", |rv, cx| {
                    let found = rv.step_find(false);
                    rv.found(found, cx)
                }))
                .child(step("dfind-next", "↓", |rv, cx| {
                    let found = rv.step_find(true);
                    rv.found(found, cx)
                }))
                .child(chip("dfind-close", "✕", false).on_click(click(|rv, cx| {
                    rv.close_find();
                    cx.notify();
                }))),
        )
    }
}

impl Review {
    /// The rows of the open diff that hold the find text, and where each change begins.
    pub fn recompute_matches(&mut self) {
        self.matches = match &self.data {
            Some(data) => rows::matching_rows(data, &self.rows, &self.dterms),
            None => Vec::new(),
        };
        self.match_at = self.match_at.min(self.matches.len().saturating_sub(1));
        let mut changes = Vec::new();
        let mut inside = false;
        for (ix, row) in self.rows.iter().enumerate() {
            match row {
                Row::Thread(_) | Row::Request(_) | Row::Draft(_) | Row::Note(_) | Row::Composer => {
                    continue;
                }
                row if rows::is_change(row) => {
                    if !inside {
                        changes.push(ix);
                    }
                    inside = true;
                }
                _ => inside = false,
            }
        }
        self.changes = changes;
        self.change_at = self.change_at.filter(|&at| at < self.changes.len());
        self.recompute_stack_matches();
    }

    /// Scrolls the diff so `row` sits a few lines below the top.
    pub fn scroll_to_row(&mut self, row: usize) {
        self.open_active();
        let row = row + self.list_base();
        self.diff_list.scroll_to(gpui::ListOffset {
            item_ix: row.saturating_sub(3),
            offset_in_item: px(0.),
        });
    }

    /// Brings `rows` into view when they are not, the first a third of the view from the top; rows
    /// already in view stay where they are. The view is counted in code rows: right after a
    /// relayout the list has not measured what lies below, so `scroll_to_reveal_item` cannot reach
    /// it.
    pub fn reveal_rows(&mut self, rows: std::ops::Range<usize>) {
        self.open_active();
        let base = self.list_base();
        let rows = rows.start + base..rows.end + base;
        let height = f32::from(self.diff_list.viewport_bounds().size.height);
        let fit = (height / f32::from(px(self.row_height()))).floor() as usize;
        let top = self.diff_list.logical_scroll_top().item_ix;
        // The top row may be cut, and threads are taller than a code row: keep a margin.
        if fit > 6 && rows.start > top && rows.end + 3 <= top + fit {
            return;
        }
        self.diff_list.scroll_to(gpui::ListOffset {
            item_ix: rows.start.saturating_sub(fit / 3),
            offset_in_item: px(0.),
        });
    }

    /// With every file in one scroll: the match of every file's after (or before) the current one.
    pub fn next_stack_match(&self, forward: bool) -> Option<usize> {
        let stack = self.stack.as_ref()?;
        let n = stack.matches.len();
        (n > 0).then(|| {
            if forward {
                (stack.match_at + 1) % n
            } else {
                (stack.match_at + n - 1) % n
            }
        })
    }

    /// The open file's next (or previous) match comes into view; false when it has none.
    pub fn step_match(&mut self, forward: bool) -> bool {
        let n = self.matches.len();
        if n == 0 {
            return false;
        }
        self.match_at = if forward {
            (self.match_at + 1) % n
        } else {
            (self.match_at + n - 1) % n
        };
        self.scroll_to_row(self.matches[self.match_at]);
        true
    }

    /// The open file's next (or previous) change comes into view; false when it has none.
    pub fn step_change(&mut self, forward: bool) -> bool {
        let n = self.changes.len();
        if n == 0 {
            return false;
        }
        let at = match (self.change_at, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(at), true) => (at + 1) % n,
            (Some(at), false) => (at + n - 1) % n,
        };
        self.change_at = Some(at);
        self.scroll_to_row(self.changes[at]);
        true
    }

    /// Find in the diff closes.
    pub fn close_find(&mut self) {
        self.dfind = None;
        self.dterms = Arc::default();
        self.recompute_matches();
    }

    /// Find in the diff looks for `text` (as typed, or as a regular expression); in the open file the
    /// first match comes into view.
    pub fn set_find(&mut self, text: String) {
        let terms = if self.dfind_regex {
            search::Terms::regex(&text)
        } else {
            Ok(search::Terms::text(&text))
        };
        self.dfind_error = terms.as_ref().err().cloned();
        self.dterms = Arc::new(terms.unwrap_or_default());
        self.dfind = Some(text);
        self.match_at = 0;
        self.recompute_matches();
        if self.stack.is_none()
            && let Some(&row) = self.matches.first()
        {
            self.scroll_to_row(row);
        }
    }

    /// ⌘F: the bar open, with what it had while it is.
    pub fn open_find(&mut self) {
        if self.dfind.is_none() {
            self.find_area = Area::default();
            self.dfind = Some(String::new());
        }
    }

    /// Find `text` (`set_find`): in the one scroll the folded files are built to be searched too, and the first
    /// match from the file in view on is where it goes.
    pub fn find(&mut self, text: String, cx: &mut Context<Self>) -> Found {
        self.set_find(text);
        self.stack.as_ref()?;
        self.build_stack(cx);
        let at = self.first_stack_match()?;
        self.stack_match(at)
    }

    /// The match after (or before) the current one: in the open file it comes into view; in the one scroll, it
    /// is where it goes.
    pub fn step_find(&mut self, forward: bool) -> Found {
        if self.stack.is_none() {
            self.step_match(forward);
            return None;
        }
        let at = self.next_stack_match(forward)?;
        self.stack_match(at)
    }

    /// `.*` or ⌥R: the text read as a regular expression, or as typed again (#113).
    pub fn toggle_find_regex(&mut self, cx: &mut Context<Self>) -> Found {
        self.dfind_regex = !self.dfind_regex;
        let text = self.dfind.clone().unwrap_or_default();
        self.find(text, cx)
    }

    /// Where a find went, for the tab to follow (`ReviewEvent::Found`); else it draws again.
    fn found(&mut self, found: Found, cx: &mut Context<Self>) {
        match found {
            Some((file, row)) => cx.emit(ReviewEvent::Found { file, row }),
            None => cx.notify(),
        }
    }
}

/// The find bar's field as one of the tab's inputs: a change finds anew, ↵ and ⇧↵ step, ⌥R reads the text as a
/// regular expression; one left empty closes.
pub fn input(review: Entity<Review>) -> Input {
    let (open, key, left, selected) = (
        review.downgrade(),
        review.downgrade(),
        review.downgrade(),
        review.downgrade(),
    );
    Input {
        id: INPUT,
        open: Rc::new(move |cx| open.upgrade().is_some_and(|rv| rv.read(cx).dfind.is_some())),
        key: Rc::new(move |event, _, cx| {
            key.update(cx, |rv, cx| {
                let key = &event.keystroke;
                let Some(mut text) = rv.dfind.clone() else {
                    return Typed::Kept;
                };
                let (typed, found) = if key.modifiers.alt && key.key == "r" {
                    (Typed::Kept, rv.toggle_find_regex(cx))
                } else {
                    match input::edit(&mut text, &mut rv.find_area.edit, key, cx) {
                        Edit::Changed => (Typed::Kept, rv.find(text, cx)),
                        Edit::Enter { shift } => (Typed::Kept, rv.step_find(!shift)),
                        Edit::Escape => (Typed::Leave, None),
                        Edit::Selected | Edit::Ignored => (Typed::Kept, None),
                    }
                };
                rv.found(found, cx);
                typed
            })
            .unwrap_or(Typed::Kept)
        }),
        left: Rc::new(move |cx| {
            left.update(cx, |rv, cx| {
                if rv.dfind.as_deref() == Some("") {
                    rv.close_find();
                    cx.notify();
                }
            })
            .ok();
        }),
        selected: Rc::new(move |cx| {
            let rv = selected.upgrade()?;
            rv.read(cx)
                .find_area
                .edit
                .selected_text()
                .map(str::to_owned)
        }),
        caret: Rc::new(|_| false),
    }
}
