//! Settings' contract: its sections in the left column, the one on with its heading and Done, its rows fading in
//! (#56, 14). The sections are offered (`add_settings_section`), each drawn by its owner in each tab: the window
//! root's own, or the rows a feature's parts put under it (`Parts::settings`), any owner's rows in their places
//! (`Rows::order`); the frame draws the page around them.

use crate::{Hook, render};
use gpui::{AnyElement, AnyView, App, Window, div, prelude::*};
use std::rc::Rc;

/// A section of Settings (`add_settings_section`): its place in the left column and its title. What it holds its
/// owner draws in each tab: the window root's own, a feature's as its parts' rows (`Parts::settings`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Section {
    /// A stable id (`agents`): what parts put rows under, what a toast's button opens.
    pub id: &'static str,
    /// Its words in the left column and over its rows.
    pub title: &'static str,
    /// Its place in the left column, the lowest first; those of one order as they were offered.
    pub order: u16,
}

/// Rows a part draws under a section of Settings (`Parts::settings`), its own or another's: Appearance holds the
/// themes' and the window root's faces and panes.
#[derive(Clone)]
pub struct Rows {
    /// The section's id (`Section::id`).
    pub section: &'static str,
    /// Their place among the section's rows, the lowest first; those of one order as their features were installed.
    pub order: u16,
    pub view: AnyView,
    /// What Set up's part for the section draws of them, if anything (Look: the Window themes, five to a row).
    pub short: Option<AnyView>,
    /// While it says so, these rows alone fill the section, given the page's height (the theme editor).
    pub fills: Option<Fills>,
    /// Esc while the section is on, before Settings closes: whether they took it (the editor's token, then the
    /// editor). It runs inside the tab's update: it reads its own state, never the tab.
    pub escape: Option<Took>,
    /// Settings closed (esc, Done, a place): what was begun in them ends (the theme being edited). Asked as the tab
    /// is drawn while Settings is shut.
    pub closed: Option<Hook>,
}

/// Whether rows fill their section now (`Rows::fills`).
pub type Fills = Rc<dyn Fn(&App) -> bool>;

/// Whether rows took a key (`Rows::escape`).
pub type Took = Rc<dyn Fn(&mut App) -> bool>;

impl Rows {
    pub fn new(section: &'static str, order: u16, view: impl Into<AnyView>) -> Self {
        Self {
            section,
            order,
            view: view.into(),
            short: None,
            fills: None,
            escape: None,
            closed: None,
        }
    }

    pub fn short(self, view: impl Into<AnyView>) -> Self {
        Self {
            short: Some(view.into()),
            ..self
        }
    }

    pub fn fills(self, now: impl Fn(&App) -> bool + 'static) -> Self {
        Self {
            fills: Some(Rc::new(now)),
            ..self
        }
    }

    pub fn escape(self, took: impl Fn(&mut App) -> bool + 'static) -> Self {
        Self {
            escape: Some(Rc::new(took)),
            ..self
        }
    }

    pub fn closed(self, end: impl Fn(&mut App) + 'static) -> Self {
        Self {
            closed: Some(Rc::new(end)),
            ..self
        }
    }
}

/// The page as the tab sees it.
pub struct Page {
    /// The sections offered, in their order.
    pub sections: Vec<Section>,
    /// The section on, by its id.
    pub on: &'static str,
    /// Its rows.
    pub content: AnyElement,
    /// Rows that fill the section take the page's height (the theme editor: its tokens scroll beside a preview that
    /// stays).
    pub editing: bool,
}

/// What a click on the frame does.
pub trait SettingsHost {
    /// A section in the left column, by its id.
    fn section(&self, id: &'static str, window: &mut Window, cx: &mut App);
    /// Done: Settings closes.
    fn done(&self, window: &mut Window, cx: &mut App);
}

/// Draws the page: the frame's.
pub trait SettingsRenderer {
    fn page(&self, page: Page, host: Rc<dyn SettingsHost>) -> AnyElement;
}

/// The page, drawn by the frame installed.
pub fn page(page: Page, host: Rc<dyn SettingsHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn SettingsRenderer>(cx) {
        Some(renderer) => renderer.page(page, host),
        None => div().into_any_element(),
    }
}
