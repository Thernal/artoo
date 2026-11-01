//! The window's body drawn: the row under the title bar — the rail, the spaces' menu, the page, the stripe, which zen
//! slides away and back (#56, 16) —, zen's strips and its line of keys, and the splitters between panes. What fills
//! them, and where a pane's width is kept, are the tab's.

use artoo_shell_api::frame::{Body, FrameRenderer, OnDown, Split, handle_width};
use artoo_ui_kit::{gap, keys, motion, px, theme};
use gpui::{AnyElement, CursorStyle, IntoElement, MouseButton, div, prelude::*};

/// The renderer, installed by `install`.
pub(crate) struct Renderer;

impl FrameRenderer for Renderer {
    fn body(&self, body: Body) -> AnyElement {
        self::body(body)
    }

    fn handle(&self, split: Split, active: bool, down: OnDown) -> AnyElement {
        self::handle(split, active, down).into_any_element()
    }

    fn zen_strip(
        &self,
        id: &'static str,
        shown: bool,
        moving: bool,
        room: f32,
        strip: AnyElement,
    ) -> AnyElement {
        self::zen_strip(id, shown, moving, room, strip)
    }

    fn zen_keys(&self, one: bool) -> AnyElement {
        self::zen_keys(one).into_any_element()
    }
}

/// The gap after a pane (right of it, or below when vertical), which resizes the pane; `active` while it is dragged.
fn handle(split: Split, active: bool, down: OnDown) -> impl IntoElement {
    let group = split.group();
    let line = div()
        .rounded_full()
        .when(active, |s| s.bg(theme::splitter()))
        .group_hover(group, |s| s.bg(theme::splitter()));
    let gap = div()
        .group(group)
        .flex_none()
        .flex()
        .items_center()
        .justify_center();
    let gap = if split.vertical() {
        gap.h(px(handle_width()))
            .w_full()
            .cursor(CursorStyle::ResizeUpDown)
            .child(line.h(px(2.)).w(px(48.)))
    } else {
        gap.w(px(handle_width()))
            .h_full()
            .cursor(CursorStyle::ResizeLeftRight)
            .child(line.w(px(2.)).h(px(48.)))
    };
    gap.on_mouse_down(MouseButton::Left, move |e, window, cx| down(e, window, cx))
}

/// A strip over or under the page that zen takes away or brings — the page's bar, the item's
/// line, zen's own: it folds to nothing and fades as the sides slide (#56, 16; the developer,
/// 2026-10-23: the top went at once). `room` is the most it is tall; `moving` while zen begins or ends.
fn zen_strip(
    id: &'static str,
    shown: bool,
    moving: bool,
    room: f32,
    strip: AnyElement,
) -> AnyElement {
    motion::toward(
        div()
            .flex_none()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(strip),
        id,
        shown,
        moving,
        motion::BASE,
        move |s, v| s.max_h(px(room * v)).opacity(v),
    )
    .into_any_element()
}

/// Zen's line at the foot of the diff: the keys of a review, from the keyboard alone; `one` while it shows one file
/// at a time.
fn zen_keys(one: bool) -> impl IntoElement {
    // Keys are split by spaces: "[ ]" is two caps.
    let pair = |spec: &'static str, what: &'static str| {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .children(spec.split(' ').map(|key| keys::caps(key, keys::On::Plain)))
            .child(what)
    };
    div()
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .justify_center()
        .gap(px(16.))
        .px(px(12.))
        .py(px(6.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(pair("[ ]", "files"))
        .child(pair("v", "reviewed, next"))
        .child(pair("⌥↓", "next request"))
        .child(pair("c", "comment"))
        .child(pair(
            "⌥⌘.",
            if one {
                "every file"
            } else {
                "one file at a time"
            },
        ))
        .child(pair("⌘.", "leave zen"))
}

/// The row.
fn body(body: Body) -> AnyElement {
    let (zen, moving) = (body.zen, body.zen_moving);
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .pl(px(gap()))
        // The stripe sits on the window's right edge; zen has none.
        .pr(px(if zen { gap() } else { 0. }))
        .pb(px(gap()))
        // Zen slides the rail away and back (#56, 16).
        .children(body.rail.map(|rail| {
            motion::toward(
                div().flex_none().flex().overflow_hidden().child(rail),
                "rail-zen",
                !zen,
                moving,
                motion::BASE,
                |rail, v| rail.max_w(px(48. * v)).opacity(v),
            )
        }))
        .children(body.spaces)
        .child(body.page)
        .children(body.stripe_gap)
        .children(body.stripe.map(|stripe| {
            motion::toward(
                div().flex_none().flex().overflow_hidden().child(stripe),
                "stripe-zen",
                !zen,
                moving,
                motion::BASE,
                |stripe, v| stripe.max_w(px(44. * v)).opacity(v),
            )
        }))
        .into_any_element()
}
