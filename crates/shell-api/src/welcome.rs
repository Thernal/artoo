//! The welcome's contract (#54): the window with no repository open — the ways in, a way's card in their place, the
//! repositories opened lately. What a way does and its card are the tab's; the frame draws the page.

use crate::render;
use gpui::{AnyElement, App, Window, div, prelude::*};
use std::path::PathBuf;
use std::rc::Rc;

/// The page as the tab sees it.
pub struct Page {
    /// A way's card (connect, clone), in place of the ways.
    pub card: Option<AnyElement>,
    /// The repositories opened lately.
    pub recent: Vec<PathBuf>,
}

/// What a click on it does.
pub trait WelcomeHost {
    /// Open a repository: the folder picker.
    fn open(&self, window: &mut Window, cx: &mut App);
    /// Connect GitLab or GitHub (`connect`), or clone from a URL: its card.
    fn card(&self, connect: bool, window: &mut Window, cx: &mut App);
    /// A repository opened lately.
    fn recent(&self, path: PathBuf, window: &mut Window, cx: &mut App);
}

/// Draws the page: the frame's.
pub trait WelcomeRenderer {
    fn page(&self, page: Page, host: Rc<dyn WelcomeHost>) -> AnyElement;
}

/// The page, drawn by the frame installed.
pub fn page(page: Page, host: Rc<dyn WelcomeHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn WelcomeRenderer>(cx) {
        Some(renderer) => renderer.page(page, host),
        None => div().into_any_element(),
    }
}
