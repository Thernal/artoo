//! The Changes list's path filter (⌥⌘P): its field over the files, one of the tab's inputs (`input`). A press in it
//! or ⌥⌘P gives it the keyboard; esc lets it go, what was typed still filtering (esc again clears it, the tab's).

use crate::review::Review;
use artoo_shell_api::{Input, Typed, input_box};
use artoo_ui_kit::input::{self, Edit};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme};
use gpui::{Context, Div, Entity, MouseButton, Stateful, div, prelude::*};
use std::rc::Rc;

impl Review {
    /// ⌥⌘P: the filter open to take the keyboard, the Changes section with it (`keep`: as the user leaves it, not
    /// zen's for a while).
    pub fn open_filter(&mut self, keep: bool, cx: &mut Context<Self>) {
        self.filter_open = true;
        self.open_changes(true, keep, false, cx);
    }

    /// The field over the files, `active` with the keyboard: its text or a placeholder.
    pub fn filter_field(&self, active: bool, cx: &Context<Self>) -> Stateful<Div> {
        let el = div()
            .id("file-filter")
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .mx(px(6.))
            .mb(px(6.))
            .h(px(26.))
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
            .child(self.filter_area.field(
                &self.pfilter,
                active,
                "Filter by path  ⌥⌘P",
                false,
                |rv: &mut Review| Some(&mut rv.filter_area),
                cx,
            ));
        let Some(frame) = self.frame.as_ref().and_then(std::rc::Weak::upgrade) else {
            return el;
        };
        let this = cx.entity().downgrade();
        input_box(el, INPUT, &frame).on_mouse_down(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |rv, cx| {
                rv.filter_open = true;
                cx.notify()
            })
            .ok();
        })
    }
}

/// The path filter's field among the tab's inputs.
pub const INPUT: &str = artoo_review_api::FILTER_INPUT;

/// The path filter's field as one of the tab's inputs: what is typed filters the files; esc lets it go.
pub fn input(review: Entity<Review>) -> Input {
    let (open, key, left, selected) = (
        review.downgrade(),
        review.downgrade(),
        review.downgrade(),
        review.downgrade(),
    );
    Input {
        id: INPUT,
        open: Rc::new(move |cx| open.upgrade().is_some_and(|rv| rv.read(cx).filter_open)),
        key: Rc::new(move |event, _, cx| {
            key.update(cx, |rv, cx| {
                let mut text = std::mem::take(&mut rv.pfilter);
                let edit = input::edit(&mut text, &mut rv.filter_area.edit, &event.keystroke, cx);
                rv.pfilter = text;
                cx.notify();
                match edit {
                    Edit::Escape => Typed::Leave,
                    _ => Typed::Kept,
                }
            })
            .unwrap_or(Typed::Kept)
        }),
        left: Rc::new(move |cx| {
            left.update(cx, |rv, cx| {
                rv.filter_open = false;
                cx.notify();
            })
            .ok();
        }),
        selected: Rc::new(move |cx| {
            let rv = selected.upgrade()?;
            rv.read(cx)
                .filter_area
                .edit
                .selected_text()
                .map(str::to_owned)
        }),
        caret: Rc::new(|_| false),
    }
}
