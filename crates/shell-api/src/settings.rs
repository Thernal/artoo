//! Settings' contract: its sections in the left column, the one on with its heading and Done, its rows fading in
//! (#56, 14). What each section holds is the tab's, and the features' rows (`Section` at the crate's root); the frame
//! draws the page around them.

use crate::render;
use gpui::{AnyElement, App, Window, div, prelude::*};
use std::rc::Rc;

/// The parts of Settings, in its left column.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Diff,
    Appearance,
    General,
    Repository,
    Requests,
    Agents,
    Keyboard,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Diff,
        Section::Appearance,
        Section::General,
        Section::Repository,
        Section::Requests,
        Section::Agents,
        Section::Keyboard,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Diff => "Diff",
            Section::Appearance => "Appearance",
            Section::General => "General",
            Section::Repository => "Repository",
            Section::Requests => "Merge requests",
            Section::Agents => "Agents",
            Section::Keyboard => "Keyboard",
        }
    }
}

/// The page as the tab sees it.
pub struct Page {
    /// The section on.
    pub on: Section,
    /// Its rows.
    pub content: AnyElement,
    /// The theme editor takes the page's height: its tokens scroll beside a preview that stays.
    pub editing: bool,
}

/// What a click on the frame does.
pub trait SettingsHost {
    /// A section in the left column.
    fn section(&self, section: Section, window: &mut Window, cx: &mut App);
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
