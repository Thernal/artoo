//! Commit search: the field above the commit list and its keyboard handling; what it finds is the history's
//! (`artoo_branches::find`).

use super::Workspace;
use super::input::{self, Edit};
use super::typing::{self, Input};
use artoo_review::find::BarEvent;
use gpui::{App, Context, KeyDownEvent, prelude::*};

impl Workspace {
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

    /// The find bar above the diff (`Review::render_find_bar`), its field the tab's.
    pub(super) fn render_dfind(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let text = self.review.read(cx).dfind.clone()?;
        let active = self.has_keyboard(Input::DiffFind, cx);
        let field = self
            .field_text(&text, active, "Find in this diff", cx)
            .into_any_element();
        let this = cx.weak_entity();
        let on = std::rc::Rc::new(move |event, cx: &mut App| {
            this.update(cx, |this, cx| match event {
                BarEvent::Step { forward } => this.step_match(forward, cx),
                BarEvent::Regex => this.toggle_dfind_regex(cx),
                BarEvent::Close => this.close_dfind(cx),
            })
            .ok();
        });
        let bar = self.review.read(cx).render_find_bar(field, active, on)?;
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
