//! Where a field's keys and text live in the workspace: which field has them, its text and look, and
//! the caret placed by a click. The field itself — drawing, editing, the caret — is the kit's
//! (`artoo_ui_kit::input`), re-exported here.

use super::{Workspace, theme};
use crate::textedit::TextEdit;
pub(super) use artoo_ui_kit::input::*;
use gpui::{Context, Div, WeakEntity, div, prelude::*};
use std::rc::Rc;

/// Where a click in the field showing `text` goes: the caret to it, ⇧ or a drag selecting, a
/// double click taking the word.
fn place_field(text: &str, this: WeakEntity<Workspace>) -> Place {
    let text = text.to_owned();
    Rc::new(move |at, shift, clicks, drag, lower, cx| {
        this.update(cx, |this, cx| {
            sync(&text, &mut this.field_edit);
            this.field_goal = None;
            this.field_edit.click(at, clicks, shift || drag);
            if clicks == 1 && !shift && !drag {
                this.field_edit.set_downstream(lower);
            }
            cx.notify();
        })
        .ok();
    })
}

impl Workspace {
    /// The inside of a one-line field: its text with the caret and selection while it has the
    /// keyboard (`active`), the text alone while not, or the placeholder.
    pub(super) fn field_text(
        &self,
        text: &str,
        active: bool,
        placeholder: &'static str,
        cx: &Context<Self>,
    ) -> Div {
        self.field_look(
            text,
            active,
            placeholder,
            Look::default(),
            cx.entity().downgrade(),
        )
    }

    /// `field_text` drawn as `look` says (wrapping, or masked), for a view without its context.
    pub(super) fn field_look(
        &self,
        text: &str,
        active: bool,
        placeholder: &'static str,
        look: Look,
        this: WeakEntity<Self>,
    ) -> Div {
        let row = div().flex_1().min_w_0().flex().items_center();
        if text.is_empty() {
            let shown = super::keys::text(placeholder).into_owned();
            return row.children(active.then(caret)).child(
                cut_unless_wrapped(look.wrap, placeholder, shown).text_color(theme::faint()),
            );
        }
        // Left, its text stays, dimmed, without a caret (#168).
        if !active {
            let shown = if look.mask {
                "•".repeat(text.chars().count())
            } else {
                text.to_owned()
            };
            return row.child(
                cut_unless_wrapped(look.wrap, placeholder, shown).text_color(theme::muted()),
            );
        }
        let look = Look {
            lines: look.wrap.then(|| self.field_lines.clone()),
            ..look
        };
        row.child(text_body(
            text,
            &self.field_edit,
            look,
            self.field_scroll.clone(),
            Some(place_field(text, this)),
        ))
    }

    /// A field takes the keyboard: its caret starts at the end of its text.
    pub(super) fn reset_field_edit(&mut self) {
        self.field_edit = TextEdit::default();
        self.field_pick = 0;
        self.field_closed = None;
        self.field_scroll.set(0.);
    }
}
