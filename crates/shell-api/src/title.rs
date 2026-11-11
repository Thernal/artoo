//! The title bar's contract, the window's and never the page's (#70): the menu's fold, back and forward, the project
//! (or the tabs, in Tabs mode), search, the items features put there, then Settings. The tab says what it shows and
//! answers its clicks; the frame draws it.

use crate::render;
use gpui::{AnyElement, App, Pixels, Point, Window, div, prelude::*};
use std::rc::Rc;

/// What the title bar shows.
pub struct TitleBar {
    /// A repository is open: the menu's fold and search are drawn.
    pub open: bool,
    /// The project's name; None in Tabs mode, where the tabs take its place (D21).
    pub project: Option<String>,
    pub tabs: Option<AnyElement>,
    /// The menu of spaces is folded to icons.
    pub folded: bool,
    /// Whether there is somewhere to go back and forward to.
    pub back: bool,
    pub forward: bool,
    /// Where ‹ goes, for its tip (`Back to !415`, #194).
    pub back_to: Option<String>,
    /// Narrow: search is its icon (#80).
    pub compact: bool,
    /// What features and the tab put before Settings: their items, the fetch, the refresh status.
    pub items: Vec<AnyElement>,
}

/// What a click on the title bar does.
pub trait TitleHost {
    fn fold(&self, window: &mut Window, cx: &mut App);
    fn back(&self, window: &mut Window, cx: &mut App);
    /// ‹ right-clicked at `at`: the last places behind (#194).
    fn back_menu(&self, at: Point<Pixels>, window: &mut Window, cx: &mut App);
    fn forward(&self, window: &mut Window, cx: &mut App);
    /// The project's right click, the rail's menu, at `at`.
    fn project_menu(&self, at: Point<Pixels>, window: &mut Window, cx: &mut App);
    /// The project clicked: a repository to open.
    fn projects(&self, window: &mut Window, cx: &mut App);
    fn search(&self, window: &mut Window, cx: &mut App);
    fn settings(&self, window: &mut Window, cx: &mut App);
}

/// Draws the title bar: the frame's.
pub trait TitleRenderer {
    fn render(&self, bar: TitleBar, host: Rc<dyn TitleHost>) -> AnyElement;
}

/// The title bar, drawn by the frame installed.
pub fn title_bar(bar: TitleBar, host: Rc<dyn TitleHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn TitleRenderer>(cx) {
        Some(renderer) => renderer.render(bar, host),
        None => div().into_any_element(),
    }
}
