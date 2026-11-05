//! The lists of the window beyond the commit list: the filter fields, and the changed files — the review draws them
//! as a folder tree or a flat list (`artoo_review::changes_view`) with their path filter, the tab says whether the
//! filter has the keyboard. Designed in review round 1 (A9, A10, A11, A13); the branches' list is Branches' root page
//! (`roots.rs`, #72).

use super::input::{self, Edit};
use super::px;
use super::typing::{self, Input};
use super::{Workspace, theme};
use gpui::{ClickEvent, Context, KeyDownEvent, div, prelude::*};

impl Workspace {
    /// The one-line field of `field` in a list's head, inset from the list's edges.
    pub(super) fn render_field(
        &self,
        id: &'static str,
        field: Field,
        placeholder: &'static str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        self.field_box(id, field, placeholder, cx)
            .mx(px(6.))
            .mb(px(6.))
    }

    /// The one-line field of `field`, like the commit search: its text or a placeholder; a click
    /// gives it the keyboard.
    pub(super) fn field_box(
        &self,
        id: &'static str,
        field: Field,
        placeholder: &'static str,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let active = self.field == Some(field);
        let (text, look) = match field {
            Field::Branches => (&self.bfilter, input::Look::default()),
            Field::Keys => (&self.keys_filter, input::Look::default()),
            Field::Label => (&self.label_name, input::Look::default()),
        };
        let el = div()
            .id(id)
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .h(px(26.))
            .rounded(px(super::ROW_RADIUS))
            .border_2()
            .border_color(if active {
                theme::focus()
            } else {
                theme::island_border()
            })
            .bg(theme::base())
            .cursor_text()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.start_field(field, cx)))
            .child(
                super::icons::icon("search")
                    .size(px(14.))
                    .text_color(theme::faint()),
            )
            .child(self.field_look(text, active, placeholder, look, cx.entity().downgrade()));
        typing::field_box_out(el, field, cx)
    }

    pub(super) fn start_field(&mut self, field: Field, cx: &mut Context<Self>) {
        if self.field != Some(field) {
            self.reset_field_edit();
        }
        // A composer or a find open beside it keeps its text, not the keyboard (#168).
        self.take_keyboard(Input::Field, cx);
        self.field = Some(field);
        cx.notify();
    }

    /// Typing into the field with the keyboard; esc lets it go. Returns whether it took the key.
    pub(super) fn filter_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(field) = self.field else {
            return false;
        };
        if field == Field::Label {
            match input::edit(
                &mut self.label_name,
                &mut self.field_edit,
                &event.keystroke,
                cx,
            ) {
                Edit::Enter { .. } => self.save_label(cx),
                Edit::Escape => self.close_label(cx),
                Edit::Changed | Edit::Selected | Edit::Ignored => {}
            }
            cx.notify();
            return true;
        }
        // A root list's filter keeps ↑ ↓ for the list it filters (#72).
        if field == Field::Branches
            && self.at_root_list()
            && matches!(event.keystroke.key.as_str(), "up" | "down")
        {
            self.step_pick(event.keystroke.key == "down", cx);
            return true;
        }
        // Typed, the keyboard filter matches by words again.
        if field == Field::Keys {
            self.keys_exact = false;
        }
        let text = match field {
            Field::Branches => &mut self.bfilter,
            Field::Keys => &mut self.keys_filter,
            Field::Label => &mut self.label_name,
        };
        let edit = input::edit(text, &mut self.field_edit, &event.keystroke, cx);
        match edit {
            // What was typed stays and still filters; esc again clears it (`clear_filter`).
            Edit::Escape => self.leave_input(cx),
            Edit::Enter { .. } if field == Field::Branches && self.at_root_list() => {
                self.open_pick(cx);
                return true;
            }
            Edit::Changed | Edit::Selected | Edit::Enter { .. } | Edit::Ignored => {}
        }
        cx.notify();
        true
    }
}

/// A filter field that can take the keyboard.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Branches,
    /// The filter of Settings → Keyboard.
    Keys,
    /// The name of a project or a chat being given (`labeling.rs`).
    Label,
}

// ---- changed files ----------------------------------------------------------------------

impl Workspace {
    pub(super) fn set_file_view(&mut self, list: bool, cx: &mut Context<Self>) {
        if self.view_options(cx).list_files != list {
            self.rv().set_list_files(list, cx);
            self.view_options(cx).save();
            cx.notify();
        }
    }

    pub(super) fn toggle_dir(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.rv().flip_dir(path, cx) {
            self.tree_opening(path, cx);
        }
        cx.notify();
    }
}
