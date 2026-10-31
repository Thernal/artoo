//! The card that comes up when the last file of a diff is marked reviewed: what to do next, from the keyboard —
//! approve the request, the next one to review (a request, or a branch), back to your branch. What the next is and
//! what each does are the features': the requests' over a request's diff, the branches' over a branch's; each gives
//! a `DoneLook` and answers a `DoneEvent`.

use crate::format::plural;
use crate::{ISLAND_RADIUS, ROW_RADIUS, hover, keys, px, theme};
use gpui::{AnyElement, App, ClickEvent, FontWeight, div, prelude::*};
use std::rc::Rc;

/// What the card says.
pub struct DoneLook {
    /// The files reviewed, and the request they are of.
    pub files: usize,
    pub request: Option<u64>,
    /// "Approve !12", while you may and have not.
    pub approve: Option<String>,
    /// What is next to review, and its key.
    pub next: Option<(String, &'static str)>,
}

/// A click on the card.
#[derive(Clone, Copy)]
pub enum DoneEvent {
    Close,
    Approve,
    Next,
    /// Back to your branch, from a request.
    Leave,
}

/// What answers the card's clicks.
pub type OnDone = Rc<dyn Fn(DoneEvent, &mut App)>;

/// The card, at the diff's bottom right corner.
pub fn render_done_card(look: DoneLook, on: OnDone) -> AnyElement {
    let button = |id: &'static str, label: String, key: &'static str, primary: bool| {
        div()
            .id(id)
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(primary, |s| s.bg(theme::added()).text_color(theme::base()))
            .when(!primary, |s| s.bg(theme::hover()).text_color(theme::text()))
            .hover(|s| s.opacity(0.85))
            .child(hover::cut("label", label))
            .child(keys::caps(
                key,
                if primary {
                    keys::On::Filled
                } else {
                    keys::On::Plain
                },
            ))
    };
    let current = look.request;
    let (close, approve, next, leave) = (on.clone(), on.clone(), on.clone(), on);
    div()
        .absolute()
        .bottom(px(16.))
        .right(px(16.))
        .max_w(px(460.))
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .rounded(px(ISLAND_RADIUS))
        .border_2()
        .border_color(theme::added())
        .bg(theme::panel())
        .shadow_lg()
        .font_family(theme::ui_font())
        .text_size(px(12.))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::added())
                        .child(match current {
                            Some(iid) => {
                                format!("All {} of !{iid} reviewed ✓", plural(look.files, "file"))
                            }
                            None => format!("All {} reviewed ✓", plural(look.files, "file")),
                        }),
                )
                .child(
                    div()
                        .id("done-card-close")
                        .px_1()
                        .rounded(px(4.))
                        .text_color(theme::faint())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                        .child("esc")
                        .on_click(move |_: &ClickEvent, _, cx| close(DoneEvent::Close, cx)),
                ),
        )
        .children(look.approve.map(|label| {
            button("done-approve", label, "a", false)
                .on_click(move |_: &ClickEvent, _, cx| approve(DoneEvent::Approve, cx))
        }))
        .children(look.next.clone().map(|(label, key)| {
            button("done-next", label, key, true)
                .on_click(move |_: &ClickEvent, _, cx| next(DoneEvent::Next, cx))
        }))
        .children(current.map(|_| {
            button("done-leave", "Back to your branch".to_owned(), "⌥⌘L", false)
                .on_click(move |_: &ClickEvent, _, cx| leave(DoneEvent::Leave, cx))
        }))
        .children(look.next.is_none().then(|| {
            div()
                .text_color(theme::muted())
                .child(if current.is_some() {
                    "Nothing left to review: every other merge request is reviewed too."
                } else {
                    "Nothing left to review: every other branch is reviewed too."
                })
        }))
        .into_any_element()
}
