//! The window's tabs' contract (Design flow 02, #49): what a window's tabs show of each tab, and how the frame draws
//! them — the project rail on the window's left edge, or the tabs in the title bar (D12, X10). What a click does is
//! the tab's (`TabsHost`).

use crate::render;
use gpui::{AnyElement, App, Pixels, Point, Window, div, prelude::*};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// What a window's tabs need to know about each tab.
#[derive(Clone)]
pub struct TabInfo {
    pub title: String,
    pub news: bool,
    pub active: bool,
    /// The repository, for Reveal in Finder and a new window.
    pub root: Option<PathBuf>,
    /// Its unread notifications, counted on the rail.
    pub unread: usize,
}

pub type TabModel = Rc<RefCell<Vec<TabInfo>>>;

/// What a click on the window's tabs does.
pub trait TabsHost {
    fn select(&self, ix: usize, window: &mut Window, cx: &mut App);
    fn close(&self, ix: usize, window: &mut Window, cx: &mut App);
    /// Tab `from` dragged onto tab `to`.
    fn moved(&self, from: usize, to: usize, window: &mut Window, cx: &mut App);
    /// Tab `ix`'s right click, at `at`.
    fn menu(&self, ix: usize, at: Point<Pixels>, window: &mut Window, cx: &mut App);
    /// +: a repository to open.
    fn add(&self, window: &mut Window, cx: &mut App);
}

/// Draws the window's tabs: the frame's.
pub trait TabsRenderer {
    fn rail(&self, tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>) -> AnyElement;
    fn title_tabs(&self, tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>) -> AnyElement;
}

/// The project rail, drawn by the frame installed.
pub fn rail(tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn TabsRenderer>(cx) {
        Some(renderer) => renderer.rail(tabs, host),
        None => div().into_any_element(),
    }
}

/// The tabs in the title bar, drawn by the frame installed.
pub fn title_tabs(tabs: Vec<TabInfo>, host: Rc<dyn TabsHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn TabsRenderer>(cx) {
        Some(renderer) => renderer.title_tabs(tabs, host),
        None => div().into_any_element(),
    }
}
