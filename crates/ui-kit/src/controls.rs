//! Controls a feature's view shares with the frame: a bar's button, a tool window's foot and its list's pieces (a
//! group's heading, a comment as an entry), rows breathing while a list loads, what follows the pointer while
//! something is dragged, the words for showing a file where it lies.

use super::{ROW_RADIUS, hover, keys, motion, px, theme, words};
use artoo_core::platform;
use gpui::{Context, Div, FontWeight, div, prelude::*};

/// Finder with it selected, as the rail's project menu words it off macOS.
pub const REVEAL: &str = if platform::MAC {
    "Reveal in Finder"
} else {
    "Show in its folder"
};

/// What follows the pointer while a space or a tab is dragged: its name.
pub struct DragLabel(pub String);

impl Render for DragLabel {
    fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.))
            .py(px(4.))
            .rounded(px(ROW_RADIUS))
            .bg(theme::selected())
            .border_1()
            .border_color(theme::island_border())
            .text_size(px(13.))
            .text_color(theme::text())
            .child(self.0.clone())
    }
}

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

/// A group's heading in a tool window's list; `keys` says how to walk it.
pub fn group_heading(text: String, keys: Option<&'static str>) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .px(px(14.))
        .pt(px(10.))
        .pb(px(4.))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::muted())
        .child(div().flex_1().child(text.to_uppercase()))
        .children(keys.map(|k| keys::caps(k, keys::On::Plain)))
}

/// A comment as one entry of a tool window's list: a dot in its colour, where it is, two lines of it.
pub fn comment_item(
    id: (&'static str, usize),
    tone: gpui::Rgba,
    where_: String,
    body: String,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .mx(px(6.))
        .px(px(8.))
        .py(px(4.))
        .overflow_hidden()
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_none().size(px(6.)).rounded_full().bg(tone))
                .child(
                    hover::cut("where", where_)
                        .flex_1()
                        .font_family(theme::code_font())
                        .text_size(px(11.))
                        .text_color(theme::faint()),
                ),
        )
        .child(
            div().pl(px(14.)).child(
                hover::cut_to("body", body, 2)
                    .text_size(px(12.))
                    .text_color(theme::muted()),
            ),
        )
}

/// Rows still loading: two grey bars each, breathing as the diff's shimmer does.
pub fn skeleton(id: &'static str, rows: usize) -> gpui::AnyElement {
    const LONG: [f32; 6] = [62., 48., 70., 55., 66., 44.];
    const SHORT: [f32; 6] = [40., 52., 34., 46., 38., 50.];
    motion::breathe(
        id,
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .children((0..rows).map(|n| {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .child(bar(LONG[n % 6], 10.))
                    .child(bar(SHORT[n % 6], 7.))
            })),
    )
}

fn bar(percent: f32, height: f32) -> Div {
    div()
        .w(gpui::relative(percent / 100.))
        .h(px(height))
        .rounded_full()
        .bg(theme::hover())
}
