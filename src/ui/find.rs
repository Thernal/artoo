//! Commit search: the field above the commit list and its keyboard handling; what it finds is the history's
//! (`artoo_branches::find`).

use super::input::{self, Edit};
use super::px;
use super::typing::{self, Input};
use super::{Workspace, theme};
use artoo_ui_kit::controls::chip;
use gpui::{App, ClickEvent, Context, KeyDownEvent, div, prelude::*};

impl Workspace {
    /// Rows in the commit list.
    pub(super) fn commit_rows(&self, cx: &App) -> usize {
        self.history.read(cx).commit_rows()
    }

    /// The index in `commits` shown at list row `row`.
    pub(super) fn commit_at(&self, row: usize, cx: &App) -> usize {
        self.history.read(cx).commit_at(row)
    }

    /// The list row of commit `ix`, if it is shown.
    pub(super) fn row_of(&self, ix: usize, cx: &App) -> Option<usize> {
        self.history.read(cx).row_of(ix)
    }

    pub(super) fn refilter(&mut self, cx: &mut App) {
        self.history.update(cx, |h, _| h.refilter());
    }

    pub(super) fn start_find(&mut self, cx: &mut Context<Self>) {
        self.settings_open = false;
        self.history.update(cx, |h, _| h.open_find());
        self.take_keyboard(Input::Part(artoo_branches::find::INPUT), cx);
        cx.notify();
    }

    pub(super) fn close_find(&mut self, cx: &mut Context<Self>) {
        self.history.update(cx, |h, _| h.close_find());
        cx.notify();
    }

    pub(super) fn step_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.review.read(cx).stack.is_some() {
            if let Some(at) = self.review.read(cx).next_stack_match(forward) {
                self.go_to_stack_match(at, cx);
            }
            return;
        }
        if self.review.update(cx, |rv, _| rv.step_match(forward)) {
            cx.notify();
        }
    }

    /// F7 / ⇧F7: the next (or previous) change in the open file.
    pub(super) fn step_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        // A document read as a page: its changed blocks (#11).
        if self.step_page(forward, cx) {
            return;
        }
        if self.review.update(cx, |rv, _| rv.step_change(forward)) {
            cx.notify();
        }
    }

    /// A key: to the input with the keyboard (#168), else the keys of what is on screen.
    pub(super) fn find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_keyboard(cx);
        match self.typing(cx) {
            Some(Input::FindFiles) => return self.ffind_key(event, cx),
            Some(Input::Palette) => return self.palette_key(event, window, cx),
            Some(Input::OpenAsk) => return self.open_ask_key(event, cx),
            Some(Input::AgentPick) => return self.agent_pick_key(event, cx),
            Some(Input::Overlay) => return self.overlay_key(event, window, cx),
            Some(Input::Beside) => return self.beside_key(event, cx),
            Some(Input::Merge) => return self.merge_key(event, cx),
            Some(Input::Reactions) => return self.reaction_key(event, cx),
            Some(Input::Submit) => return self.submit_key(event, cx),
            Some(Input::NewRequest) => return self.newreq_key(event, cx),
            Some(Input::GoLine) => return self.goline_key(event, cx),
            Some(Input::Tool) => return self.tool_key(event, window, cx),
            Some(Input::Compose) => return self.compose_key(event, cx),
            Some(Input::Field) => {
                self.filter_key(event, cx);
                return;
            }
            Some(Input::DiffFind) => return self.dfind_key(event, cx),
            Some(Input::Part(id)) => return self.part_input_key(id, event, window, cx),
            None => {}
        }
        let key = &event.keystroke;
        // ⌫ on the draft picked in the Agents window deletes it; Undo is in its toast (#76).
        if key.key == "backspace"
            && self.tool() == Some(super::tools::Tool::Agents)
            && let Some(id) = self.agent_row.filter(|id| {
                self.comments
                    .iter()
                    .any(|c| c.id == *id && c.state == crate::review::State::Draft)
            })
        {
            return self.delete_draft(id, cx);
        }
        // Esc closes the innermost thing — a card, menu or popover, the find bar, a filter, the
        // tool window with the keyboard — then goes one page up (#71).
        if key.key == "escape" {
            if self.review.read(cx).done_card {
                self.review.update(cx, |rv, _| rv.done_card = false);
                cx.notify();
            } else if self.label_edit.is_some() {
                self.close_label(cx);
            } else if self.overlay_open() {
                self.close_overlay(cx);
            } else if self.shortcuts {
                self.shortcuts = false;
                cx.notify();
            } else if self.ctx_menu.is_some() {
                self.close_menu(cx);
            } else if self.review.read(cx).goline.is_some() {
                self.close_goline(cx);
                cx.notify();
            } else if self.review.read(cx).dfind.is_some() {
                self.close_dfind(cx);
            } else if self.history.read(cx).find.text.is_some() {
                self.close_find(cx);
            } else if self.side_drawer() {
                self.close_drawer(cx);
            } else if !self.nav.picked.is_empty() {
                self.nav.picked.clear();
                cx.notify();
            } else if self.close_setup(cx) || (self.settings_open && self.theme_edit_escape(cx)) {
            } else if self.settings_open {
                self.settings_open = false;
                cx.notify();
            } else if self.note_sel.take().is_some()
                || self.review.update(cx, |rv, _| rv.sel.take().is_some())
            {
                cx.notify();
            } else if self.tool().is_some() && self.zone == super::zones::Zone::Tool {
                self.set_tool(None, cx);
            } else if self.clear_filter(cx) {
                cx.notify();
            } else if self.unpick_compare(cx) {
            } else {
                self.go_up(cx);
            }
            return;
        }
        // 0…9 answer the agent's form in view (#10 C).
        if key.modifiers.number_of_modifiers() == 0
            && let Some(digit) = key.key.parse::<usize>().ok().filter(|d| *d < 10)
        {
            self.answer_by_key(digit, cx);
        }
    }

    /// Esc after esc: the filter that was left with text, of the list on screen, is cleared.
    fn clear_filter(&mut self, cx: &mut App) -> bool {
        if !self.review.read(cx).pfilter.is_empty()
            && self.review.read(cx).diff.is_some()
            && self
                .space()
                .is_some_and(|s| s != super::spaces::Space::Code)
        {
            self.review.update(cx, |rv, _| rv.pfilter.clear());
            return true;
        }
        if !self.bfilter.is_empty() && self.at_root_list() {
            self.bfilter.clear();
            return true;
        }
        false
    }

    /// Typing into the find bar over the diff; esc lets the keyboard go, what was typed kept.
    fn dfind_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let Some(mut text) = self.review.read(cx).dfind.clone() else {
            return;
        };
        // ⌥R: the text as a regular expression, or as typed again (#113).
        if key.modifiers.alt && key.key == "r" {
            return self.toggle_dfind_regex(cx);
        }
        match input::edit(&mut text, &mut self.field_edit, key, cx) {
            Edit::Changed => self.set_dfind(text, cx),
            Edit::Selected => cx.notify(),
            Edit::Enter { shift } => self.step_match(!shift, cx),
            Edit::Escape => self.leave_input(cx),
            Edit::Ignored => {}
        }
    }

    // ---- find in the diff ----------------------------------------------------------------

    /// ⌘F: a find bar over the open diff, like the page find of a browser or an IDE.
    pub(super) fn start_dfind(&mut self, cx: &mut Context<Self>) {
        if self.review.read(cx).dfind.is_none() {
            self.reset_field_edit();
        }
        self.settings_open = false;
        if self.review.read(cx).dfind.is_none() {
            self.review
                .update(cx, |rv, _| rv.dfind = Some(String::new()));
        }
        self.take_keyboard(Input::DiffFind, cx);
        cx.notify();
    }

    /// `.*` in ⌘F's bar: its text read as a regular expression, or as typed.
    pub(super) fn toggle_dfind_regex(&mut self, cx: &mut Context<Self>) {
        self.review
            .update(cx, |rv, _| rv.dfind_regex = !rv.dfind_regex);
        let text = self.review.read(cx).dfind.clone().unwrap_or_default();
        self.set_dfind(text, cx);
    }

    pub(super) fn close_dfind(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| rv.close_find());
        cx.notify();
    }

    pub(super) fn set_dfind(&mut self, text: String, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| rv.set_find(text));
        if self.review.read(cx).stack.is_some() {
            // The folded files are built to be searched too.
            self.build_stack(cx);
            if let Some(at) = self.review.read(cx).first_stack_match() {
                self.go_to_stack_match(at, cx);
            }
        }
        cx.notify();
    }

    /// The find bar above the diff: the text, "2 of 4 lines", previous, next, close.
    pub(super) fn render_dfind(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let text = self.review.read(cx).dfind.clone()?;
        let active = self.has_keyboard(Input::DiffFind, cx);
        let count = if let Some(error) = &self.review.read(cx).dfind_error {
            format!("Not a regular expression: {error}")
        } else if self.review.read(cx).dterms.is_empty() {
            String::new()
        } else if let Some(count) = self.review.read(cx).stack_match_count() {
            count
        } else if self.review.read(cx).matches.is_empty() {
            "No matches".to_owned()
        } else {
            format!(
                "{} of {} lines",
                self.review.read(cx).match_at + 1,
                self.review.read(cx).matches.len()
            )
        };
        let step = |id: &'static str, label: &'static str, forward: bool| {
            chip(id, label, false).on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.step_match(forward, cx)),
            )
        };
        let bar = div()
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
                    .rounded(px(super::ROW_RADIUS))
                    .border_2()
                    .border_color(if active {
                        theme::focus()
                    } else {
                        theme::island_border()
                    })
                    .bg(theme::base())
                    .cursor_text()
                    .child(
                        super::icons::icon("search")
                            .size(px(14.))
                            .text_color(theme::faint()),
                    )
                    .child(self.field_text(&text, active, "Find in this diff", cx))
                    .child({
                        use super::hover::Hover;
                        chip("dfind-regex", ".*", self.review.read(cx).dfind_regex)
                            .tip("Regular expression  ⌥R")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.toggle_dfind_regex(cx)
                            }))
                    }),
            )
            .child(
                super::hover::cut("find-count", count.clone())
                    .text_size(px(12.))
                    .text_color(
                        if count == "No matches" || self.review.read(cx).dfind_error.is_some() {
                            theme::removed()
                        } else {
                            theme::muted()
                        },
                    ),
            )
            .child(step("dfind-prev", "↑", false))
            .child(step("dfind-next", "↓", true))
            .child(
                chip("dfind-close", "✕", false)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_dfind(cx))),
            );
        Some(typing::input_box(bar, Input::DiffFind, cx))
    }

    /// Reads the paths of every loaded commit (`History::index_paths`).
    pub(super) fn index_paths(&mut self, cx: &mut Context<Self>) {
        self.history.update(cx, |h, cx| h.index_paths(cx));
    }

    /// The search field, its qualifier tags and the count line.
    pub(super) fn render_find(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let active = self.has_keyboard(Input::Part(artoo_branches::find::INPUT), cx);
        let acts = self.history_acts(cx);
        let el = artoo_branches::find::field(
            self.history.read(cx),
            active,
            self.history.downgrade(),
            &acts,
        );
        typing::input_box(el, Input::Part(artoo_branches::find::INPUT), cx)
    }
}
