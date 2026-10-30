//! Controls a feature's view shares with the frame: a bar's button, a tool window's foot.

use super::{ROW_RADIUS, keys, px, theme, words};
use gpui::{Div, FontWeight, div, prelude::*};

/// A button of the bar: its label and its key as caps; the primary one filled.
pub fn bar_button(
    id: &'static str,
    label: &str,
    key: &'static str,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .text_size(px(13.))
        .when(primary, |s| {
            s.bg(theme::accent())
                .text_color(theme::on_accent())
                .font_weight(FontWeight::MEDIUM)
                .hover(|s| s.opacity(0.9))
        })
        .when(!primary, |s| {
            s.border_1()
                .border_color(theme::island_border())
                .hover(|s| s.bg(theme::hover()))
        })
        .child(words::say(label).into_owned())
        .when(!key.is_empty(), |s| {
            s.child(keys::caps(
                key,
                if primary {
                    keys::On::Filled
                } else {
                    keys::On::Plain
                },
            ))
        })
}

/// The foot of a tool window, under its body that scrolls: where its buttons are.
pub fn tool_foot() -> Div {
    div()
        .flex_none()
        .px(px(14.))
        .py(px(10.))
        .border_t_1()
        .border_color(theme::island_border())
}
