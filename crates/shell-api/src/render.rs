//! The frame's render contracts (the kits' `ComponentRenderer`; conventions-kit, components rule 8): a piece of the
//! window's frame the tab draws without knowing the frame. Its params, its callbacks (a host), its renderer trait and
//! a facade function are in this contract; the drawing is `artoo-shell`'s, whose `install` provides each renderer as
//! a GPUI global (the kits' `LocalXRenderer provides XRendererImpl`). With none installed a facade draws nothing, as
//! the kits' preview default.

use gpui::{App, Global};
use std::rc::Rc;

/// The renderer `R` installed.
struct Installed<R: ?Sized>(Rc<R>);

impl<R: ?Sized + 'static> Global for Installed<R> {}

/// `renderer` draws `R`'s piece from now on, in every window.
pub fn provide<R: ?Sized + 'static>(renderer: Rc<R>, cx: &mut App) {
    cx.set_global(Installed(renderer));
}

/// The renderer of `R`, when one is installed.
pub fn renderer<R: ?Sized + 'static>(cx: &App) -> Option<Rc<R>> {
    cx.try_global::<Installed<R>>()
        .map(|installed| installed.0.clone())
}
