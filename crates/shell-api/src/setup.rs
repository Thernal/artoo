//! Set up's contract (#143 B): the title and Later, the parts' tabs with a check on a part done, the part in the column
//! between, the count and Back · Skip · Next. Which parts are offered and what each holds are the tab's; the frame
//! draws the page around them.

use crate::render;
use gpui::{AnyElement, App, Window, div, prelude::*};
use std::rc::Rc;

/// The page as the tab sees it.
pub struct Page {
    /// Each part's name, and whether it is done as its Settings section says.
    pub tabs: Vec<(&'static str, bool)>,
    /// The part on.
    pub at: usize,
    pub body: AnyElement,
}

/// What a click on the frame does.
pub trait SetupHost {
    /// Part `at`: a tab, Back, Skip, Next.
    fn go(&self, at: usize, window: &mut Window, cx: &mut App);
    /// Later and Done: what was under it comes back.
    fn close(&self, window: &mut Window, cx: &mut App);
}

/// Draws the page: the frame's.
pub trait SetupRenderer {
    fn page(&self, page: Page, host: Rc<dyn SetupHost>) -> AnyElement;
}

/// The page, drawn by the frame installed.
pub fn page(page: Page, host: Rc<dyn SetupHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn SetupRenderer>(cx) {
        Some(renderer) => renderer.page(page, host),
        None => div().into_any_element(),
    }
}
