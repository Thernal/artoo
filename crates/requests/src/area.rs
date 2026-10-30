//! A card's text field (Merge's message, Submit review's summary, the New merge request form's), kept by the card rather than in the tab's one
//! field: its caret, selection and undo, its layout as drawn, its scroll and the x ↑ ↓ keep.

use crate::requests::Requests;
use artoo_core::textedit::TextEdit;
use artoo_ui_kit::input::{self, Lines, Look};
use gpui::{Context, Div, Pixels};
use std::cell::Cell;
use std::rc::Rc;

pub(crate) struct Area {
    pub(crate) edit: TextEdit,
    pub(crate) lines: Lines,
    scroll: Rc<Cell<f32>>,
    pub(crate) goal: Option<Pixels>,
}

impl Area {
    /// The caret at the end of `text`.
    pub(crate) fn new(text: String) -> Self {
        Area {
            edit: TextEdit::new(text),
            lines: Lines::default(),
            scroll: Rc::new(Cell::new(0.)),
            goal: None,
        }
    }

    /// The caret in `text`; at its end until the field is edited.
    pub(crate) fn caret(&self, text: &str) -> usize {
        if self.edit.text() == text {
            self.edit.caret()
        } else {
            text.len()
        }
    }

    /// The field showing `text`, wrapping or on one line: as edited while it has the keyboard
    /// (`active`), else as it stands; a click places the caret, ⇧ or a drag selecting, a double
    /// click taking the word. `of` finds the area again when the click lands.
    pub(crate) fn field(
        &self,
        text: &str,
        active: bool,
        placeholder: &'static str,
        wrap: bool,
        of: fn(&mut Requests) -> Option<&mut Area>,
        cx: &Context<Requests>,
    ) -> Div {
        let look = Look {
            wrap,
            ..Default::default()
        };
        input::field(text, active, placeholder, &look, || {
            let this = cx.entity().downgrade();
            let shown = text.to_owned();
            let place: input::Place = Rc::new(move |at, shift, clicks, drag, lower, cx| {
                this.update(cx, |rq, cx| {
                    if let Some(area) = of(rq) {
                        input::sync(&shown, &mut area.edit);
                        area.goal = None;
                        area.edit.click(at, clicks, shift || drag);
                        if clicks == 1 && !shift && !drag {
                            area.edit.set_downstream(lower);
                        }
                        cx.notify();
                    }
                })
                .ok();
            });
            let look = Look {
                lines: wrap.then(|| self.lines.clone()),
                ..look.clone()
            };
            input::text_body(text, &self.edit, look, self.scroll.clone(), Some(place))
        })
    }
}
