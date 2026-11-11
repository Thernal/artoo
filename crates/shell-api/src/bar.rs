//! The page bar's contract (#71): one 44 px line over every page — the path ending in the item's id, each part above
//! it a way up; "‹ Inbox" while a page reached from another space is on screen; the page's view switch; its actions;
//! a request's public edge. The tab says what it shows and answers its clicks; the frame draws it.

use crate::{MenuItem, render};
use gpui::{AnyElement, App, Pixels, Point, Window, div, prelude::*};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// An action on the bar (#186): what it draws, its shorter form, and its row in the bar's `⋯` when the bar has no
/// room for it. Labels shorten first, then actions fold (#187).
pub struct BarAction {
    /// Its name among the page's actions: the width it was drawn at is kept by it.
    pub id: &'static str,
    pub element: AnyElement,
    /// Narrower, what it says in its tooltip: `Submit review` its icon, `Approve 1/2` without its key.
    pub short: Option<AnyElement>,
    /// Its row in `⋯`; `None` for one that never folds.
    pub fold: Option<MenuItem>,
}

impl BarAction {
    /// One that never folds.
    pub fn new(id: &'static str, element: impl IntoElement) -> Self {
        Self {
            id,
            element: element.into_any_element(),
            short: None,
            fold: None,
        }
    }

    /// Its shorter form, drawn before anything folds when the bar has no room for every label.
    pub fn short(self, element: impl IntoElement) -> Self {
        Self {
            short: Some(element.into_any_element()),
            ..self
        }
    }

    /// Its row in `⋯` when there is no room for it.
    pub fn folds(self, item: MenuItem) -> Self {
        Self {
            fold: Some(item),
            ..self
        }
    }
}

/// The bar's widths as last drawn, in pixels (#186): kept by the tab, measured by the frame. What they leave the
/// actions decides which fold into `⋯`, the last first.
#[derive(Default)]
pub struct BarRoom {
    /// The whole bar.
    pub width: Cell<f32>,
    /// The page's view switch.
    pub view: Cell<f32>,
    /// Each action by its id, as last drawn on the bar; and its shorter form.
    pub actions: RefCell<HashMap<&'static str, f32>>,
    pub shorts: RefCell<HashMap<&'static str, f32>>,
    /// The left — the path and the title — as last drawn, and as wide as it is whole, once seen with room to spare;
    /// shorter than the room kept for it, the actions have the rest.
    pub left: Cell<f32>,
    pub left_whole: Cell<Option<f32>>,
    /// The page the left was seen whole on: another forgets it.
    page: RefCell<String>,
}

impl BarRoom {
    /// The bar is drawn for `page` (its space and title): the left's whole width is forgotten when it is another.
    pub fn page(&self, page: String) {
        if *self.page.borrow() != page {
            self.page.replace(page);
            self.left_whole.set(None);
        }
    }
}

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
    pub actions: Vec<BarAction>,
    /// Rows only `⋯` holds, after the actions folded into it: Open in GitLab, Copy link. `⋯` shows only when an action
    /// folds.
    pub more: Vec<MenuItem>,
    /// Its widths as last drawn.
    pub room: Rc<BarRoom>,
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
    /// `⋯` clicked: what folded into it, then `more`, a rule between, at `at`.
    fn menu(
        &self,
        at: Point<Pixels>,
        groups: Vec<Vec<MenuItem>>,
        window: &mut Window,
        cx: &mut App,
    );
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
