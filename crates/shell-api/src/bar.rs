//! The page bar's contract (#71): one 44 px line over every page — the path ending in the item's id, each part above
//! it a way up; "‹ Inbox" while a page reached from another space is on screen; the page's view switch; its actions;
//! a request's public edge. The tab says what it shows and answers its clicks; the frame draws it.

use crate::render;
use gpui::{AnyElement, App, Pixels, Point, Window, div, prelude::*};
use std::rc::Rc;

/// A part of the path above the page.
pub struct Crumb {
    pub label: String,
    pub mono: bool,
    /// It goes up to its page.
    pub link: bool,
    /// A folder of Code: a click lists what is beside it, a right click its menu (#132, B).
    pub folder: bool,
    /// Its list is open.
    pub open: bool,
    /// A note for the agent waits on it: its dot.
    pub noted: bool,
    /// What is beside it, listed under it.
    pub list: Option<AnyElement>,
}

/// The bar of the page on screen.
pub struct PageBar {
    /// The space a page was reached from: "‹ Inbox".
    pub from: Option<&'static str>,
    /// Narrow: the path is ‹, one level up (#80).
    pub up: bool,
    pub crumbs: Vec<Crumb>,
    /// The page's title, with what its long hover says.
    pub title: AnyElement,
    pub count: Option<String>,
    pub view: Option<AnyElement>,
    pub actions: Vec<AnyElement>,
    /// Public: what is done here is seen on GitLab (#71).
    pub public: bool,
}

/// What a click on the bar does.
pub trait BarHost {
    /// ‹ Inbox: back where the page was reached from.
    fn back(&self, window: &mut Window, cx: &mut App);
    /// ‹: one level up.
    fn up(&self, window: &mut Window, cx: &mut App);
    /// Crumb `n` clicked: up to its page.
    fn up_to(&self, n: usize, window: &mut Window, cx: &mut App);
    /// Folder crumb `n` clicked: what is beside it.
    fn folder(&self, n: usize, window: &mut Window, cx: &mut App);
    /// Folder crumb `n`'s right click, at `at`.
    fn folder_menu(&self, n: usize, at: Point<Pixels>, window: &mut Window, cx: &mut App);
}

/// Draws the bar: the frame's.
pub trait BarRenderer {
    fn render(&self, bar: PageBar, host: Rc<dyn BarHost>) -> AnyElement;
}

/// The bar, drawn by the frame installed.
pub fn page_bar(bar: PageBar, host: Rc<dyn BarHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn BarRenderer>(cx) {
        Some(renderer) => renderer.render(bar, host),
        None => div().into_any_element(),
    }
}
