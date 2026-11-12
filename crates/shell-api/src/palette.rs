//! Search everywhere's window's contract (#52): its tabs, each named with what it found, the field, the rows grouped,
//! the foot with the tab's key; the same window picks a comparison's refs, a theme, a recent place, a bookmark, a face.
//! What it lists, what the field holds and what a row does are the tab's; the frame draws it.

use crate::render;
use gpui::{AnyElement, App, SharedString, Window, div, prelude::*};
use std::rc::Rc;

/// A row of the list.
pub struct Row {
    /// The group it is listed under: a heading where it changes.
    pub group: SharedString,
    pub label: String,
    pub detail: String,
    /// Its key as it is now, none empty.
    pub key: String,
    /// A file's path: its icon before it (#95).
    pub icon: Option<String>,
    /// An icon by the kit's name before it, a command's or a place's (#192).
    pub glyph: Option<&'static str>,
    /// The dots after its words (#194).
    pub dots: Vec<Dot>,
    /// A file or a repository: its name, then its folder right after it, cut first.
    pub file: bool,
    /// A face, drawn in itself (#24).
    pub face: Option<SharedString>,
    /// A swatch before it: a ground and two lines, 0xRRGGBBAA (#204).
    pub swatch: Option<[u32; 3]>,
    /// The one in force: a check at its end (#204).
    pub checked: bool,
    /// A word on it while it is picked, a click on it the host's `aside` (a theme's Edit, #204).
    pub aside: Option<&'static str>,
}

/// A dot after a row's words, as the menu's open rows wear them (#194).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dot {
    /// Comments not sent yet, in the agents' colour.
    Drafts,
    /// Something happened since it was last seen, in the accent.
    News,
}

/// A tab of search everywhere.
pub struct Tab {
    pub label: &'static str,
    /// What it found for the text.
    pub count: Option<usize>,
    pub lit: bool,
}

/// The window as the tab sees it.
pub struct PaletteView {
    pub title: Option<String>,
    /// Search everywhere's tabs.
    pub tabs: Option<Vec<Tab>>,
    /// `>` moved it to Actions: its mark before the field.
    pub actions: bool,
    pub field: AnyElement,
    pub rows: Vec<Row>,
    /// The row picked.
    pub at: usize,
    /// What it says when nothing is listed.
    pub empty: &'static str,
    /// The tab on with its key: "Files · ⌘P".
    pub foot: Option<String>,
    /// Dropped from a button rather than over the window's middle (#204): the button, in the window, and how tall
    /// the list may grow under it. It is narrow, its rows short, its groups counted, the field's keys at its right.
    pub under: Option<(gpui::Bounds<gpui::Pixels>, gpui::Pixels)>,
    /// The field's keys at its right, when dropped: `↑↓ preview · ↵ · esc`.
    pub hint: Option<&'static str>,
    /// The list's scroll, and whether it goes to the row picked (the keys moved it, or it opened).
    pub scroll: (gpui::ScrollHandle, bool),
}

/// What a click on the window does.
pub trait PaletteHost {
    /// Row `ix`: what it does.
    fn run(&self, ix: usize, window: &mut Window, cx: &mut App);
    /// Outside it: it closes.
    fn close(&self, window: &mut Window, cx: &mut App);
    /// Tab `ix` of search everywhere.
    fn scope(&self, ix: usize, window: &mut Window, cx: &mut App);
    /// The word on row `ix` (`Row::aside`).
    fn aside(&self, ix: usize, window: &mut Window, cx: &mut App);
}

/// Draws the window: the frame's.
pub trait PaletteRenderer {
    fn palette(&self, view: PaletteView, host: Rc<dyn PaletteHost>) -> AnyElement;
}

/// The window, over everything, drawn by the frame installed.
pub fn palette(view: PaletteView, host: Rc<dyn PaletteHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn PaletteRenderer>(cx) {
        Some(renderer) => renderer.palette(view, host),
        None => div().into_any_element(),
    }
}
