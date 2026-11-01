//! The keyboard card's contract: every key of the app in one place, grouped by what one is doing. The groups are
//! the tab's; the frame draws the card.

use crate::{Hook, render};
use gpui::{AnyElement, App, div, prelude::*};
use std::rc::Rc;

/// A group of the card: its title, and (keys, what they do).
pub type Group = (&'static str, &'static [(&'static str, &'static str)]);

/// What the card says of your keys (the tab's keymap).
pub struct Keys {
    /// The presets yours may start from (#54), each with whether they do.
    pub presets: Vec<(&'static str, bool)>,
    /// The keys changed in Settings → Keyboard: each action's words and its keys as drawn, none when it has none.
    pub changes: Vec<(String, Vec<String>)>,
}

/// The preset picked, by its place in `Keys::presets`.
pub type OnPreset = Rc<dyn Fn(usize, &mut App)>;

/// Draws it: the frame's.
pub trait ShortcutsRenderer {
    fn card(&self, groups: &'static [Group], keys: Keys, close: Hook, pick: OnPreset)
    -> AnyElement;
}

/// The card over the window, drawn by the frame installed: `close` closes it (a click beside it, Close), `pick`
/// picks a preset.
pub fn card(
    groups: &'static [Group],
    keys: Keys,
    close: Hook,
    pick: OnPreset,
    cx: &App,
) -> AnyElement {
    match render::renderer::<dyn ShortcutsRenderer>(cx) {
        Some(renderer) => renderer.card(groups, keys, close, pick),
        None => div().into_any_element(),
    }
}
