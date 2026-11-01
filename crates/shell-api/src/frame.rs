//! The window's body's contract: the row under the title bar — the rail, the spaces' menu, the page, the stripe,
//! which zen slides away and back (#56, 16) —, zen's strips and its line of keys, and the splitters between panes.
//! What fills them, and where a pane's width is kept, are the tab's; the frame draws them.

use crate::render;
use artoo_ui_kit::{GAP, flat};
use gpui::{AnyElement, App, CursorStyle, MouseDownEvent, Pixels, Point, Window, div, prelude::*};
use std::rc::Rc;

/// What a press on a handle does: the tab starts the drag.
pub type OnDown = Rc<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

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

    /// The hover group its handle draws in.
    pub fn group(self) -> &'static str {
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

/// Draws the body's pieces: the frame's.
pub trait FrameRenderer {
    fn body(&self, body: Body) -> AnyElement;
    fn handle(&self, split: Split, active: bool, down: OnDown) -> AnyElement;
    fn zen_strip(
        &self,
        id: &'static str,
        shown: bool,
        moving: bool,
        room: f32,
        strip: AnyElement,
    ) -> AnyElement;
    fn zen_keys(&self, one: bool) -> AnyElement;
}

/// The row under the title bar, a repository open, drawn by the frame installed.
pub fn body(body: Body, cx: &App) -> AnyElement {
    match render::renderer::<dyn FrameRenderer>(cx) {
        Some(renderer) => renderer.body(body),
        None => div().into_any_element(),
    }
}

/// The gap after a pane, which resizes it, drawn by the frame installed; `active` while it is dragged.
pub fn handle(split: Split, active: bool, down: OnDown, cx: &App) -> AnyElement {
    match render::renderer::<dyn FrameRenderer>(cx) {
        Some(renderer) => renderer.handle(split, active, down),
        None => div().into_any_element(),
    }
}

/// A strip zen takes away or brings, drawn by the frame installed; `room` is the most it is tall.
pub fn zen_strip(
    id: &'static str,
    shown: bool,
    moving: bool,
    room: f32,
    strip: AnyElement,
    cx: &App,
) -> AnyElement {
    match render::renderer::<dyn FrameRenderer>(cx) {
        Some(renderer) => renderer.zen_strip(id, shown, moving, room, strip),
        None => div().into_any_element(),
    }
}

/// Zen's line of keys, drawn by the frame installed; `one` while it shows one file at a time.
pub fn zen_keys(one: bool, cx: &App) -> AnyElement {
    match render::renderer::<dyn FrameRenderer>(cx) {
        Some(renderer) => renderer.zen_keys(one),
        None => div().into_any_element(),
    }
}
