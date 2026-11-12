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
    /// A file or a repository: its name, then its folder right after it, cut first.
    pub file: bool,
    /// A face, drawn in itself (#24).
    pub face: Option<SharedString>,
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
}

/// What a click on the window does.
pub trait PaletteHost {
    /// Row `ix`: what it does.
    fn run(&self, ix: usize, window: &mut Window, cx: &mut App);
    /// Outside it: it closes.
    fn close(&self, window: &mut Window, cx: &mut App);
    /// Tab `ix` of search everywhere.
    fn scope(&self, ix: usize, window: &mut Window, cx: &mut App);
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
