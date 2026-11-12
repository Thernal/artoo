//! The right edge's contract (Design `design/shell/`, #49): an icon stripe that opens one tool window at a time beside
//! the page, and a tool window — a title with a faint count, buttons in its header, the body, a foot. Which tools a
//! space has, what each shows and what opening one does are the tab's; the frame draws them.

use crate::render;
use gpui::{AnyElement, App, Rgba, SharedString, Window, div, prelude::*};
use std::rc::Rc;

/// What a tool window shows: a title with a faint count, buttons in its header, the body (which
/// scrolls itself) and an optional foot.
pub struct ToolView {
    pub title: SharedString,
    pub count: Option<String>,
    pub actions: Vec<AnyElement>,
    pub body: AnyElement,
    pub foot: Option<AnyElement>,
}

/// A tool's button on the stripe.
pub struct Button {
    /// Its tool's name, handed back when it is clicked.
    pub id: &'static str,
    pub icon: &'static str,
    /// Its long hover, its key last: the window's title, or what it names (`Tool::heading`).
    pub tip: SharedString,
    /// Its window is open.
    pub on: bool,
    /// A count on it, in its colour.
    pub badge: Option<(String, Rgba)>,
    /// What the count is, said in its long hover (#53): "3 comments".
    pub what: String,
    /// Drawn under it: the Agents window's dot per chat that listens (#77).
    pub under: Option<AnyElement>,
}

/// What the tab does when a tool's button is clicked: its name.
pub type OnTool = Rc<dyn Fn(&'static str, &mut Window, &mut App)>;

/// Where a tool window sits.
pub struct Place {
    pub width: f32,
    /// A narrow window has no room beside the page: the tool window floats over its right edge.
    pub over: bool,
    /// The splitter before it, beside the page (none when it floats), and its width.
    pub handle: Option<AnyElement>,
    pub handle_width: f32,
    /// Its island's border: the keyboard's zone shows on it.
    pub border: Rgba,
    /// Open; closed, it is drawn on its way out (#56, 3).
    pub shown: bool,
    /// It opened or closed a moment ago.
    pub recent: bool,
}

/// What the tab does when a tool window's ✕ is clicked.
pub type OnClose = Rc<dyn Fn(&mut Window, &mut App)>;

/// Draws the right edge: the frame's.
pub trait ToolsRenderer {
    fn stripe(&self, buttons: Vec<Button>, toggle: OnTool) -> AnyElement;
    fn window(&self, view: ToolView, place: Place, close: OnClose) -> AnyElement;
}

/// The stripe of `buttons`, drawn by the frame installed.
pub fn stripe(buttons: Vec<Button>, toggle: OnTool, cx: &App) -> AnyElement {
    match render::renderer::<dyn ToolsRenderer>(cx) {
        Some(renderer) => renderer.stripe(buttons, toggle),
        None => div().into_any_element(),
    }
}

/// Tool window `view` at its place, drawn by the frame installed; `close` hides it (its ✕).
pub fn window(view: ToolView, place: Place, close: OnClose, cx: &App) -> AnyElement {
    match render::renderer::<dyn ToolsRenderer>(cx) {
        Some(renderer) => renderer.window(view, place, close),
        None => div().into_any_element(),
    }
}
