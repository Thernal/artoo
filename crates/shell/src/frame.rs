//! The window's body drawn: the row under the title bar — the rail, the spaces' menu, the page, the stripe, which zen
//! slides away and back (#56, 16) —, zen's strips and its line of keys, and the splitters between panes. What fills
//! them, and where a pane's width is kept, are the tab's.

use artoo_ui_kit::{GAP, flat, gap, keys, motion, px, theme};
use gpui::{
    AnyElement, App, CursorStyle, IntoElement, MouseButton, MouseDownEvent, Pixels, Point, Window,
    div, prelude::*,
};

/// A pane boundary that can be dragged.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Sidebar,
    /// The tool window's left edge: dragging left widens it.
    Tool,
    /// The list of the Inbox, Branches and Merge requests beside its preview.
    List,
}

impl Split {
    pub fn vertical(self) -> bool {
        false
    }

    /// The least and the most it is wide.
    pub fn bounds(self) -> (f32, f32) {
        match self {
            Split::Sidebar => (220., 720.),
            Split::Tool => (260., 640.),
            Split::List => (300., 900.),
        }
    }

    /// The pointer coordinate along the axis it resizes.
    pub fn axis(self, position: Point<Pixels>) -> f32 {
        f32::from(if self.vertical() {
            position.y
        } else {
            position.x
        })
    }

    fn group(self) -> &'static str {
        match self {
            Split::Sidebar => "split-sidebar",
            Split::Tool => "split-tool",
            Split::List => "split-list",
        }
    }
}

/// A boundary being dragged.
#[derive(Clone, Copy)]
pub struct Drag {
    pub split: Split,
    /// Pointer coordinate along the split's axis when the drag began.
    pub origin: f32,
    /// Pane size when the drag began.
    pub size: f32,
}

impl Drag {
    /// The pane's size with the pointer at `position`, in the interface's units as the widths are (`zoom`): the edge
    /// stays under the pointer at any interface size.
    pub fn size_at(&self, position: Point<Pixels>, zoom: f32) -> f32 {
        let (min, max) = self.split.bounds();
        let moved = (self.split.axis(position) - self.origin) / zoom;
        let moved = if self.split == Split::Tool {
            -moved
        } else {
            moved
        };
        (self.size + moved).clamp(min, max)
    }

    /// The pointer while it is dragged.
    pub fn cursor(&self) -> CursorStyle {
        if self.split.vertical() {
            CursorStyle::ResizeUpDown
        } else {
            CursorStyle::ResizeLeftRight
        }
    }
}

/// How wide a pane's handle is: the gap between islands, a thin line between flat panes. A pane
/// that slides in and out is its width and its handle, so nothing of it stays at the end.
pub fn handle_width() -> f32 {
    if flat() { 3. } else { GAP }
}

/// The gap after a pane (right of it, or below when vertical), which resizes the pane; `active` while it is dragged.
pub fn handle(
    split: Split,
    active: bool,
    down: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
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
    gap.on_mouse_down(MouseButton::Left, down)
}

/// A strip over or under the page that zen takes away or brings — the page's bar, the item's
/// line, zen's own: it folds to nothing and fades as the sides slide (#56, 16; the developer,
/// 2026-10-23: the top went at once). `room` is the most it is tall; `moving` while zen begins or ends.
pub fn zen_strip(
    id: &'static str,
    shown: bool,
    moving: bool,
    room: f32,
    strip: impl IntoElement + 'static,
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
pub fn zen_keys(one: bool) -> impl IntoElement {
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

/// The row under the title bar, a repository open.
pub struct Body {
    pub rail: Option<AnyElement>,
    pub spaces: Option<AnyElement>,
    /// The page of the space on screen (#71).
    pub page: AnyElement,
    /// The gap before the stripe, a tool window open or not (#68).
    pub stripe_gap: Option<AnyElement>,
    pub stripe: Option<AnyElement>,
    /// Zen, and whether it is beginning or ending.
    pub zen: bool,
    pub zen_moving: bool,
}

/// The row.
pub fn body(body: Body) -> AnyElement {
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
