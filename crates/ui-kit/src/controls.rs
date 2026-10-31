//! Controls a feature's view shares with the frame: a bar's button, a tool window's foot and its list's pieces (a
//! group's heading, a comment as an entry), rows breathing while a list loads, what follows the pointer while
//! something is dragged, the words for showing a file where it lies; a root list's row pieces (its unread dot, how
//! far its review is), its preview's pills and buttons, a page's message when it has nothing to show.

use super::{ROW_RADIUS, hover, icons, keys, motion, px, theme, words};
use artoo_core::platform;
use gpui::{AnyElement, Context, Div, FontWeight, Rgba, SharedString, div, prelude::*};

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

/// A small toggle in the diff toolbar.
pub fn chip(
    id: impl Into<gpui::ElementId>,
    label: impl Into<SharedString>,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .py(px(2.))
        .rounded(px(ROW_RADIUS))
        .text_size(px(12.))
        .cursor_pointer()
        .when(active, |s| {
            s.bg(theme::selected()).text_color(theme::text())
        })
        .when(!active, |s| {
            s.text_color(theme::muted())
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
        })
        .child(label.into())
}

/// Chips that belong together, on a shared rounded track.
pub fn group() -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(ROW_RADIUS + 2.))
        .bg(theme::base())
}

/// The blue dot of a row not seen yet: a request you have never opened, or one of yours a thread
/// waits on; a blank of its width otherwise.
pub fn unread_dot(on: bool) -> Div {
    div()
        .flex_none()
        .mt(px(6.))
        .size(px(7.))
        .rounded_full()
        .when(on, |s| s.bg(theme::accent()))
}

/// One message centred in the content: an icon, what is the case, why, and the way on.
pub fn empty_state(
    icon: &'static str,
    title: String,
    text: String,
    action: Option<AnyElement>,
) -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .px(px(24.))
        .child(icons::icon(icon).size(px(24.)).text_color(theme::muted()))
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(title),
        )
        // Room for three lines, so the box stays put as its words change (the developer,
        // 2026-10-23).
        .child(
            div()
                .max_w(px(400.))
                .min_h(px(19. * 3.))
                .text_size(px(13.))
                .line_height(px(19.))
                .text_center()
                .text_color(theme::muted())
                .child(text),
        )
        .children(action.map(|a| div().pt(px(6.)).child(a)))
}

/// A small rounded label in a preview's head: the pipeline, the approvals.
/// A small label in a thin ring of `color`, beside a row's title: a version's role, a commit's ref, a state.
pub fn tag(text: impl Into<SharedString>, color: Rgba) -> Div {
    div()
        .px(px(6.))
        .rounded_full()
        .border_1()
        .border_color(color)
        .text_size(px(10.))
        .text_color(color)
        .child(text.into())
}

pub fn pill(text: String, color: Rgba) -> Div {
    div()
        .flex_none()
        .px(px(8.))
        .h(px(22.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(color)
        .text_size(px(12.))
        .text_color(color)
        .child(text)
}

/// The filled button of a preview, with its key.
pub fn primary_button(id: &'static str, label: &str, key: &'static str) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(28.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(ROW_RADIUS))
        .bg(theme::accent())
        .text_color(theme::on_accent())
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(|s| s.opacity(0.9))
        .child(words::say(label).into_owned())
        .child(keys::caps(key, keys::On::Filled))
}

/// A plain outlined button.
pub fn outline_button(label: &str) -> Div {
    div()
        .flex_none()
        .h(px(28.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .cursor_pointer()
        .text_size(px(13.))
        .hover(|s| s.bg(theme::hover()))
        .child(words::say(label).into_owned())
}

/// "3/7" beside a branch or request whose review started: files reviewed of the whole diff.
pub fn progress_badge(done: u32, total: u32) -> gpui::Div {
    div()
        .flex_none()
        .px(px(5.))
        .rounded_full()
        .bg(theme::hover())
        .text_size(px(11.))
        .text_color(theme::muted())
        .child(format!("{done}/{total}"))
}

/// A small pill on a section's header (the side panel's Commits: the working tree, the search), lit while `on`.
pub fn section_chip(id: &'static str, on: bool) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .h(px(22.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(px(ROW_RADIUS))
        .text_size(px(12.))
        .text_color(if on { theme::text() } else { theme::muted() })
        .cursor_pointer()
        .when(on, |s| s.bg(theme::selected()))
        .hover(|s| s.bg(theme::hover()))
}
