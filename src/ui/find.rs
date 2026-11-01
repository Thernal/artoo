//! Commit search: the field above the commit list and its keyboard handling; what it finds is the history's
//! (`artoo_branches::find`).

use super::Workspace;
use super::typing::Input;
use artoo_review::find::Found;
use gpui::{App, Context, KeyDownEvent};

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

    /// ⌘G / ⇧⌘G: the match after or before (`Review::step_find`).
    pub(super) fn step_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        let found = self.review.update(cx, |rv, _| rv.step_find(forward));
        self.go_found(found, cx);
    }

    /// Where a find went in the one scroll (`ReviewEvent::Found`): its file the active one, the row in view.
    pub(super) fn go_found(&mut self, found: Found, cx: &mut Context<Self>) {
        if let Some((file, row)) = found {
            self.activate_file(file, cx);
            self.review.update(cx, |rv, _| rv.scroll_to_row(row));
        }
        cx.notify();
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
            Some(Input::Overlay) => return self.overlay_key(event, window, cx),
            Some(Input::Beside) => return self.beside_key(event, cx),
            Some(Input::Merge) => return self.merge_key(event, cx),
            Some(Input::Reactions) => return self.reaction_key(event, cx),
            Some(Input::Submit) => return self.submit_key(event, cx),
            Some(Input::Tool) => return self.tool_key(event, window, cx),
            Some(Input::Compose) => return self.compose_key(event, cx),
            Some(Input::Field) => {
                self.filter_key(event, cx);
                return;
            }
            Some(Input::Part(id)) => return self.part_input_key(id, event, window, cx),
            None => {}
        }
        let key = &event.keystroke;
        // ⌫ in a tool window that lists: the Agents window deletes the draft picked, Undo in its
        // toast (#76).
        if key.key == "backspace"
            && let Some(super::tools::Tool::Part(id)) = self.tool()
            && self.part_list_key(id, artoo_shell_api::ListKey::Delete, cx)
        {
            return;
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

    // ---- find in the diff ----------------------------------------------------------------

    /// ⌘F: a find bar over the open diff, like the page find of a browser or an IDE.
    pub(super) fn start_dfind(&mut self, cx: &mut Context<Self>) {
        self.settings_open = false;
        self.review.update(cx, |rv, _| rv.open_find());
        self.take_keyboard(Input::Part(artoo_review::find::INPUT), cx);
        cx.notify();
    }

    pub(super) fn close_dfind(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| rv.close_find());
        cx.notify();
    }

    /// Find `text` in the diff (`Review::find`), where it goes followed.
    pub(super) fn set_dfind(&mut self, text: String, cx: &mut Context<Self>) {
        let found = self.review.update(cx, |rv, cx| rv.find(text, cx));
        self.go_found(found, cx);
    }

    /// Reads the paths of every loaded commit (`History::index_paths`).
    pub(super) fn index_paths(&mut self, cx: &mut Context<Self>) {
        self.history.update(cx, |h, cx| h.index_paths(cx));
    }
}
