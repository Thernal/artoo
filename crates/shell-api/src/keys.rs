//! The keys in force — the app's keymap, with what Settings → Keyboard changed —, as the shell writes them beside a
//! command and at the foot of ⌘K's window. The app provides the reader once.

use gpui::{Action, App};
use std::rc::Rc;

/// The keymap as it is now.
pub trait Keys {
    /// `act`'s key, as the keymap writes it (`⇧⌘r`): the user's, else the shipped one; empty for none.
    fn now(&self, act: &dyn Action, cx: &App) -> String;
    /// `act`'s key as a foot says it: one the user changed as the keymap writes it, a shipped one as the app writes its
    /// keys, the command key first and the letter in capitals (`⌘⇧R`); empty for none.
    fn foot(&self, act: &dyn Action, cx: &App) -> String;
}

/// The keymap's reader, the app's.
pub fn provide(keys: Rc<dyn Keys>, cx: &mut App) {
    crate::render::provide::<dyn Keys>(keys, cx);
}

/// The keymap's reader, once the app has provided it.
pub fn get(cx: &App) -> Option<Rc<dyn Keys>> {
    crate::render::renderer::<dyn Keys>(cx)
}
