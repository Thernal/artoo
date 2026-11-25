//! The link under the pointer while ⌘ (Ctrl elsewhere) is held (Design round 46): it is marked where it is drawn,
//! and the tab's line at the window's foot says where a ⌘-click goes. Each place a link is drawn says when the
//! pointer is over it (`over`); every move forgets the last first (`watch`, once in the window).

use crate::{px, theme};
use gpui::{
    App, DispatchPhase, InteractiveElement, IntoElement, MouseExitEvent, MouseMoveEvent, Rgba,
    SharedString, Styled, Window, WindowId, canvas,
};
use std::cell::RefCell;

/// A link under the pointer: the spot it is drawn at, and what it points at.
#[derive(Clone, PartialEq)]
struct Over {
    window: WindowId,
    spot: SharedString,
    address: SharedString,
}

#[derive(Default)]
struct State {
    cmd: bool,
    now: Option<Over>,
    /// What the window last drew marked.
    shown: Option<Over>,
}

impl State {
    fn marked(&self) -> Option<&Over> {
        self.now.as_ref().filter(|_| self.cmd)
    }

    /// Whether what is marked changed since the last call: the window is then drawn again.
    fn settle(&mut self) -> bool {
        let now = self.marked().cloned();
        let changed = now != self.shown;
        self.shown = now;
        changed
    }
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

/// The address a request's number drawn by a feature points at, which the tab makes the host's page of:
/// `artoo-request:412`.
pub const REQUEST_SCHEME: &str = "artoo-request:";

/// Request `iid` as a link's address.
pub fn request(iid: u64) -> String {
    format!("{REQUEST_SCHEME}{iid}")
}

/// ⌘ (Ctrl elsewhere) pressed or let go, or the window left.
pub fn held(cmd: bool, window: &mut Window) {
    if with(|s| {
        s.cmd = cmd;
        s.settle()
    }) {
        window.refresh();
    }
}

/// The pointer is over the link at `spot`, pointing at `address`: said on its every move there.
pub fn over(window: &Window, spot: impl Into<SharedString>, address: impl Into<SharedString>) {
    let over = Over {
        window: window.window_handle().window_id(),
        spot: spot.into(),
        address: address.into(),
    };
    with(|s| s.now = Some(over));
}

/// Whether the link at `spot` is the one marked.
pub fn is(spot: &str) -> bool {
    with(|s| s.marked().is_some_and(|o| o.spot.as_ref() == spot))
}

/// What the link marked in `window` points at.
pub fn address(window: &Window) -> Option<SharedString> {
    let id = window.window_handle().window_id();
    with(|s| (s.marked().filter(|o| o.window == id)).map(|o| o.address.clone()))
}

/// A moved pointer is over no link until one says so; drawn again once all have, if what is marked changed.
fn moved(cmd: bool, window: &mut Window, cx: &mut App) {
    with(|s| {
        s.cmd = cmd;
        s.now = None;
    });
    window.defer(cx, |window, _| {
        if with(State::settle) {
            window.refresh();
        }
    });
}

/// Watches the window's pointer, before any link hears it: drawn once, anywhere in the window.
pub fn watch() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |_, _, window, _| {
            window.on_mouse_event(|e: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    moved(e.modifiers.secondary(), window, cx);
                }
            });
            window.on_mouse_event(|e: &MouseExitEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    moved(e.modifiers.secondary(), window, cx);
                }
            });
        },
    )
    .absolute()
    .size_0()
}

/// The tint behind a marked link.
pub fn tint() -> Rgba {
    Rgba {
        a: 0.14,
        ..theme::accent()
    }
}

/// An element that is a link, or holds one.
pub trait Marked: InteractiveElement + Styled + Sized {
    /// A link's own words: tinted and underlined while marked.
    fn link_mark(self, spot: impl Into<SharedString>, address: impl Into<SharedString>) -> Self {
        let spot: SharedString = spot.into();
        let on = is(&spot);
        let marked = self.link_over(spot, address);
        if on {
            marked.bg(tint()).rounded(px(3.)).underline()
        } else {
            marked
        }
    }

    /// A row whose ⌘-click is the link's: tinted while marked.
    fn link_row(self, spot: impl Into<SharedString>, address: impl Into<SharedString>) -> Self {
        let spot: SharedString = spot.into();
        let on = is(&spot);
        let marked = self.link_over(spot, address);
        if on { marked.bg(tint()) } else { marked }
    }

    /// Says the pointer is over the link, marking nothing itself.
    fn link_over(self, spot: impl Into<SharedString>, address: impl Into<SharedString>) -> Self {
        let (spot, address): (SharedString, SharedString) = (spot.into(), address.into());
        self.on_mouse_move(move |_, window, _| over(window, spot.clone(), address.clone()))
    }
}

impl<E: InteractiveElement + Styled> Marked for E {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_marked_without_the_key() {
        let mut state = State::default();
        assert!(!state.settle());
        state.cmd = true;
        assert!(state.marked().is_none() && !state.settle());
    }
}
