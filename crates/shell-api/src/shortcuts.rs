//! The keyboard card's contract: every key of the app in one place, grouped by what one is doing. The groups are
//! the tab's; the frame draws the card.

use crate::{Hook, render};
use gpui::{AnyElement, App, div, prelude::*};

/// A group of the card: its title, and (keys, what they do).
pub type Group = (&'static str, &'static [(&'static str, &'static str)]);

/// Draws it: the frame's.
pub trait ShortcutsRenderer {
    fn card(&self, groups: &'static [Group], close: Hook, preset: Hook, cx: &App) -> AnyElement;
}

/// The card over the window, drawn by the frame installed: `close` closes it (a click beside it, Close), `preset`
/// follows a preset picked.
pub fn card(groups: &'static [Group], close: Hook, preset: Hook, cx: &App) -> AnyElement {
    match render::renderer::<dyn ShortcutsRenderer>(cx) {
        Some(renderer) => renderer.card(groups, close, preset, cx),
        None => div().into_any_element(),
    }
}
