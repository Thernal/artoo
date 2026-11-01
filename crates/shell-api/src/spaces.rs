//! The spaces' menu's contract (#70): the four spaces of the shell — Inbox, Code, Branches and Merge requests — with
//! what waits in each and what is open under each, greyed with a line when the project cannot have one; in Menu mode
//! under the active project's row (X13), folded to icons (D2). What is open, what waits and what a click does are the
//! tab's; the frame draws it.

use crate::render;
use crate::tabs::{TabInfo, TabsHost};
use artoo_ui_kit::motion::Folds;
use artoo_ui_kit::room::Heights;
use gpui::{AnyElement, App, Pixels, Point, Window, div, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Space {
    Inbox,
    Code,
    Branches,
    Requests,
}

impl Space {
    pub const ALL: [Space; 4] = [Space::Inbox, Space::Code, Space::Branches, Space::Requests];

    pub fn label(self) -> &'static str {
        match self {
            Space::Inbox => "Inbox",
            Space::Code => "Code",
            Space::Branches => "Branches",
            Space::Requests => "Merge requests",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Space::Inbox => "inbox",
            Space::Code => "folder",
            Space::Branches => "branch",
            Space::Requests => "pull-request",
        }
    }

    /// Its key as the app writes keys (Ctrl+Shift+1…4 on Windows and Linux).
    pub fn key(self) -> &'static str {
        match self {
            Space::Inbox => "⌃⌘1",
            Space::Code => "⌃⌘2",
            Space::Branches => "⌃⌘3",
            Space::Requests => "⌃⌘4",
        }
    }

    /// Its element's id in the menu.
    pub fn id(self) -> &'static str {
        match self {
            Space::Inbox => "space-inbox",
            Space::Code => "space-code",
            Space::Branches => "space-branches",
            Space::Requests => "space-requests",
        }
    }

    /// As `layout.txt` keeps the menu's order, and features name it (`Pages::space`).
    pub fn name(self) -> &'static str {
        self.id().trim_start_matches("space-")
    }

    /// Its place in `Space::ALL`.
    pub fn ix(self) -> usize {
        match self {
            Space::Inbox => 0,
            Space::Code => 1,
            Space::Branches => 2,
            Space::Requests => 3,
        }
    }

    /// The space named `name`.
    pub fn named(name: &str) -> Option<Space> {
        Space::ALL.into_iter().find(|s| s.name() == name)
    }
}

/// The menu's spaces in the order `kept` names them (dragged there), any it leaves out after
/// them in their first order.
pub fn ordered(kept: &str) -> Vec<Space> {
    let mut out: Vec<Space> = kept
        .split(',')
        .filter_map(|n| Space::named(n.trim()))
        .collect();
    out.dedup();
    for s in Space::ALL {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

/// The menu's scroll (#146): its rows scroll once they are taller than the window — many
/// requests open under Merge requests —, and the row of the page on screen comes into view when
/// that page changes.
#[derive(Default)]
pub struct Places {
    pub scroll: gpui::ScrollHandle,
    /// The row last brought into view, by what it shows.
    pub revealed: RefCell<String>,
    /// The next frame brings the row on screen into view.
    pub reveal: Rc<Cell<bool>>,
}

/// What is open under a space, as its row in the menu: a request, a branch, Code's file.
pub struct Open {
    /// Its icon: a request's or a branch's, Code's file by its own (#95).
    pub icon: AnyElement,
    pub label: String,
    pub mono: bool,
    /// Its long hover, the whole of it.
    pub tip: String,
    /// It is the page on screen.
    pub on: bool,
    /// Picked with shift, for ⌘W.
    pub picked: bool,
    /// Agent comments not sent yet on its files.
    pub drafts: bool,
    /// Notifications of it not read yet.
    pub news: bool,
    /// The agent that gathered it (#148): its badge, and how many of its pending comments wait.
    pub agent: Option<(AnyElement, usize)>,
    /// What an agent put there for the reviewer, not opened since (#160).
    pub call: Option<String>,
}

/// A space's row, with what is open under it.
pub struct SpaceRow {
    pub space: Space,
    /// What the project lacks for it, as its line: greyed.
    pub missing: Option<&'static str>,
    /// It cannot be opened at all (no remote): no hover.
    pub inert: bool,
    /// What waits in it: its count, a badge folded.
    pub waiting: usize,
    /// What is open under it is folded away.
    pub shut: bool,
    pub opens: Vec<Open>,
    /// What is open under it is folding away or back.
    pub opens_moving: bool,
}

/// The menu as the tab sees it.
pub struct SpacesMenu<'a> {
    /// Folded to icons.
    pub folded: bool,
    /// Folding or unfolding.
    pub moving: bool,
    /// The space on screen.
    pub here: Option<Space>,
    /// The spaces, in the menu's order.
    pub rows: Vec<SpaceRow>,
    /// Menu mode: the window's projects, the spaces under the active one's row (X13).
    pub projects: Vec<(usize, TabInfo)>,
    pub tabs: Rc<dyn TabsHost>,
    /// A narrow window's side panel, folded into this column (#80).
    pub panel_icons: Option<AnyElement>,
    /// Zen: it slides away (#56, 16), `zen_moving` while it does.
    pub zen: bool,
    pub zen_moving: bool,
    pub places: &'a Places,
    pub folds: &'a Folds,
    pub heights: &'a Heights,
}

/// What a click on the menu does.
pub trait SpacesHost {
    /// A space's row: its page.
    fn go(&self, space: Space, window: &mut Window, cx: &mut App);
    /// What is open under `space` shown or hidden.
    fn toggle(&self, space: Space, window: &mut Window, cx: &mut App);
    /// `from` dragged onto `to`: it takes that place.
    fn moved(&self, from: Space, to: Space, window: &mut Window, cx: &mut App);
    /// Row `n` under `space` clicked; with shift it is picked, for ⌘W.
    fn open(&self, space: Space, n: usize, shift: bool, window: &mut Window, cx: &mut App);
    /// Row `n`'s ×.
    fn close(&self, space: Space, n: usize, window: &mut Window, cx: &mut App);
    /// Row `n`'s right click, at `at`: its link, and a request's number (#162).
    fn row_menu(
        &self,
        space: Space,
        n: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    );
}

/// Draws the menu: the frame's.
pub trait SpacesRenderer {
    fn menu(&self, menu: SpacesMenu<'_>, host: Rc<dyn SpacesHost>) -> AnyElement;
}

/// The menu, drawn by the frame installed.
pub fn menu(menu: SpacesMenu<'_>, host: Rc<dyn SpacesHost>, cx: &App) -> AnyElement {
    match render::renderer::<dyn SpacesRenderer>(cx) {
        Some(renderer) => renderer.menu(menu, host),
        None => div().into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menus_order_is_kept_by_name_and_fills_in() {
        assert_eq!(ordered(""), Space::ALL.to_vec());
        assert_eq!(
            ordered("requests,inbox"),
            [Space::Requests, Space::Inbox, Space::Code, Space::Branches]
        );
        assert_eq!(ordered("nonsense,code,code").first(), Some(&Space::Code));
    }
}
