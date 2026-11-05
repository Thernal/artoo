//! The spaces' menu's contract (#70): the four spaces of the shell — Inbox, Code, Branches and Merge requests — with
//! what waits in each and what is open under each, greyed with a line when the project cannot have one; in Menu mode
//! under the active project's row (X13), folded to icons (D2). What is open, what waits and what a click does are the
//! tab's; the frame draws it.

use crate::render;
use crate::tabs::{TabInfo, TabsHost};
use artoo_ui_kit::motion::Folds;
use artoo_ui_kit::room::Heights;
use gpui::{AnyElement, App, Pixels, Point, SharedString, Window, div, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A space, by its id (`code`): its owner offers it (`add`), `layout.txt` keeps the menu's order by it and a
/// feature's pages name the space they are in by it (`Pages::space`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Space(pub &'static str);

impl Space {
    /// Its id.
    pub fn name(self) -> &'static str {
        self.0
    }

    /// Its element's id in the menu (`space-code`).
    pub fn id(self) -> SharedString {
        format!("space-{}", self.0).into()
    }
}

/// A space as its owner offers it: its row in the menu, its place there until the developer drags it, and the
/// action that goes to it, whose key the row's hover says.
#[derive(Clone, Copy, Debug)]
pub struct SpaceInfo {
    pub space: Space,
    pub label: &'static str,
    pub icon: &'static str,
    /// Its place in the menu, the lowest first, until the developer drags it elsewhere.
    pub order: u16,
    /// The action that goes to it, by its id (`code.space`).
    pub go: &'static str,
}

/// Offers a space (its owner's `install`). An id offered already is refused, and the log says so.
pub fn add(cx: &mut App, info: SpaceInfo) {
    let registry = cx.default_global::<crate::Registry>();
    if registry.spaces.iter().any(|s| s.space == info.space) {
        crate::refuse(registry, format!("space {}", info.space.name()));
        return;
    }
    registry.spaces.push(info);
}

/// The spaces offered, in their order; those of one order as they were offered.
pub fn all(cx: &App) -> Vec<SpaceInfo> {
    let mut spaces = cx
        .try_global::<crate::Registry>()
        .map(|r| r.spaces.clone())
        .unwrap_or_default();
    spaces.sort_by_key(|s| s.order);
    spaces
}

/// The spaces `offered` in the order `kept` names them (dragged there), those it leaves out after them in their
/// own order. A name nobody offered is passed over.
pub fn ordered(kept: &str, offered: &[SpaceInfo]) -> Vec<SpaceInfo> {
    let mut out: Vec<SpaceInfo> = Vec::new();
    for name in kept.split(',').map(str::trim) {
        if let Some(info) = offered.iter().find(|s| s.space.name() == name)
            && !out.iter().any(|s| s.space == info.space)
        {
            out.push(*info);
        }
    }
    for info in offered {
        if !out.iter().any(|s| s.space == info.space) {
            out.push(*info);
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
    pub label: &'static str,
    pub icon: &'static str,
    /// The key of the action that goes to it, as the app writes keys (`⌃⌘2`); empty without one.
    pub key: String,
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

    fn info(name: &'static str, order: u16) -> SpaceInfo {
        SpaceInfo {
            space: Space(name),
            label: name,
            icon: name,
            order,
            go: name,
        }
    }

    #[test]
    fn the_menus_order_is_kept_by_name_and_fills_in() {
        let offered = [
            info("inbox", 10),
            info("code", 20),
            info("branches", 30),
            info("requests", 40),
        ];
        let names = |kept: &str| -> Vec<&str> {
            ordered(kept, &offered)
                .iter()
                .map(|s| s.space.name())
                .collect()
        };
        assert_eq!(names(""), ["inbox", "code", "branches", "requests"]);
        assert_eq!(
            names("requests,inbox"),
            ["requests", "inbox", "code", "branches"]
        );
        assert_eq!(names("nonsense,code,code").first(), Some(&"code"));
        // A space offered later, one the order was never kept with, comes after.
        let more = [offered.as_slice(), &[info("jira", 25)]].concat();
        let kept: Vec<&str> = ordered("requests,inbox,code,branches", &more)
            .iter()
            .map(|s| s.space.name())
            .collect();
        assert_eq!(kept, ["requests", "inbox", "code", "branches", "jira"]);
    }
}
