//! Settings' contract: its sections in the left column, the one on with its heading and Done, its rows fading in
//! (#56, 14). The sections are offered (`add_settings_section`), each drawn by its owner in each tab: the window
//! root's own, or the rows a feature's parts put under it (`Parts::settings`); the frame draws the page around them.

use crate::render;
use gpui::{AnyElement, App, Window, div, prelude::*};
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

/// The page as the tab sees it.
pub struct Page {
    /// The sections offered, in their order.
    pub sections: Vec<Section>,
    /// The section on, by its id.
    pub on: &'static str,
    /// Its rows.
    pub content: AnyElement,
    /// The theme editor takes the page's height: its tokens scroll beside a preview that stays.
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
