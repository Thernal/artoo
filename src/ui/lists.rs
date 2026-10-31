//! The lists of the window beyond the commit list: the filter fields, and the changed files — the review draws them
//! as a folder tree or a flat list (`artoo_review::changes_view`), the tab gives it the path filter's field and the
//! width it has. Designed in review round 1 (A9, A10, A11, A13); the branches' list is Branches' root page
//! (`roots.rs`, #72).

use super::input::{self, Edit};
use super::px;
use super::typing::{self, Input};
use super::{Workspace, theme};
use artoo_review::changes_view::ChangesLook;
use gpui::{ClickEvent, Context, KeyDownEvent, div, prelude::*};
use std::rc::Rc;

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
            Field::Files => (&self.review.read(cx).pfilter, input::Look::default()),
            Field::Token => (
                &self.token_input,
                input::Look {
                    mask: true,
                    ..Default::default()
                },
            ),
            Field::Keys => (&self.keys_filter, input::Look::default()),
            Field::ThemeValue => (&self.theme_value, input::Look::default()),
            Field::Host => (&self.welcome.host, input::Look::default()),
            Field::CloneUrl => (&self.welcome.url, input::Look::default()),
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
        if field == Field::Files {
            self.layout.show_files = true;
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
        if field == Field::ThemeValue {
            match input::edit(
                &mut self.theme_value,
                &mut self.field_edit,
                &event.keystroke,
                cx,
            ) {
                Edit::Changed => self.theme_value_changed(cx),
                Edit::Enter { .. } | Edit::Escape => self.field = None,
                Edit::Selected | Edit::Ignored => {}
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
        // The connect panel: ↵ with nothing pasted presses the focused way's button; ↑ ↓ move
        // between the ways (`zone_vertical`, #156).
        if field == Field::Token
            && self.requests.read(cx).connectable
            && !self.settings_open
            && event.keystroke.key == "enter"
            && self.token_input.trim().is_empty()
        {
            self.press_way(cx);
            return true;
        }
        // Typed, the keyboard filter matches by words again.
        if field == Field::Keys {
            self.keys_exact = false;
        }
        // The Changes list's filter is the review's: typed into a copy, put back.
        let mut files = String::new();
        let text = match field {
            Field::Branches => &mut self.bfilter,
            Field::Files => {
                files = self.review.read(cx).pfilter.clone();
                &mut files
            }
            Field::Token => &mut self.token_input,
            Field::Keys => &mut self.keys_filter,
            Field::ThemeValue => &mut self.theme_value,
            Field::Host => &mut self.welcome.host,
            Field::CloneUrl => &mut self.welcome.url,
            Field::Label => &mut self.label_name,
        };
        let welcome = self.root.is_none() && self.welcome.card.is_some();
        let edit = input::edit(text, &mut self.field_edit, &event.keystroke, cx);
        if field == Field::Files {
            self.review.update(cx, |rv, _| rv.pfilter = files);
        }
        match edit {
            // In the welcome's card, esc goes back to its choices, what was typed kept.
            Edit::Escape if welcome => {
                self.welcome.card = None;
                self.welcome.said = None;
                self.field = None;
            }
            // What was typed stays and still filters; esc again clears it (`clear_filter`).
            Edit::Escape => self.leave_input(cx),
            Edit::Enter { .. } if field == Field::Host => {
                self.start_field(Field::Token, cx);
                return true;
            }
            Edit::Enter { .. } if field == Field::Token && welcome => {
                self.welcome_connect(cx);
                return true;
            }
            Edit::Enter { .. } if field == Field::CloneUrl => {
                self.welcome_clone(cx);
                return true;
            }
            Edit::Enter { .. } if field == Field::Branches && self.at_root_list() => {
                self.open_pick(cx);
                return true;
            }
            Edit::Enter { .. } if field == Field::Token => {
                self.submit_token(cx);
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
    Files,
    /// The GitLab token being typed or pasted in the connect panel.
    Token,
    /// The filter of Settings → Keyboard.
    Keys,
    /// The value of a token in the theme editor (Settings → Appearance).
    ThemeValue,
    /// The welcome's connect card: the host (#54).
    Host,
    /// The welcome's clone card: the repository's URL (#54).
    CloneUrl,
    /// The name of a project or a chat being given (`labeling.rs`).
    Label,
}

// ---- changed files ----------------------------------------------------------------------

impl Workspace {
    pub(super) fn set_file_view(&mut self, list: bool, cx: &mut Context<Self>) {
        if self.review.read(cx).options.list_files != list {
            self.review.update(cx, |rv, _| rv.options.list_files = list);
            self.review.read(cx).options.save();
            cx.notify();
        }
    }

    pub(super) fn toggle_dir(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.review.update(cx, |rv, _| rv.collapsed.remove(path)) {
            self.tree_opening(path, cx);
        } else {
            self.review
                .update(cx, |rv, _| rv.collapsed.insert(path.to_owned()));
        }
        cx.notify();
    }

    /// The Changes section's body, the review's (`Review::render_changes`): the path filter and the width it is
    /// drawn at are the tab's.
    pub(super) fn render_changes(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let look = ChangesLook {
            measure: self.widths.measure("changes").into_any_element(),
            filter: self
                .render_field("file-filter", Field::Files, "Filter by path  ⌥⌘P", cx)
                .into_any_element(),
            counts: self.counts_fit(),
            draft: self.draft_path(cx),
        };
        let host = Rc::new(super::TabHost(cx.entity().downgrade()));
        self.review
            .update(cx, |rv, cx| rv.render_changes(look, host, cx))
    }

    /// A file's +/− shows while the list has 240 px (#79).
    fn counts_fit(&self) -> bool {
        let width = self.widths.width("changes");
        width <= 0. || width >= 240.
    }
}
