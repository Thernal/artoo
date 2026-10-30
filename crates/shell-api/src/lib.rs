//! The shell's contract (#165 stage 4): what a feature adds to the window and asks of it, so a feature knows
//! this contract and never the shell, and the shell knows no feature.
//!
//! - **Actions** (`add_action`): a command's words, the menu that shows it, its GPUI action. The app menu and ⌘K
//!   list them; the tab that has the keys runs one through its parts (`Parts::actions`).
//! - **Parts** (`add_parts`): what a feature draws in every tab — an item in the title bar, a card over the
//!   window, rows in Settings, the views a snapshot run opens —, made for each tab with its `Frame`.
//! - **`Frame`**: what a part asks of its tab: a toast, the error line, a redraw, the text selection.
//!
//! The shell calls a part from inside its own update: a part's `key`, `close`, `poll` and snapshot views never
//! ask their `Frame` (the tab is busy then, and asking it would panic); the part's listeners and tasks do.

use artoo_ui_kit::markdown::Selecting;
use gpui::{AnyView, App, Global, KeyDownEvent, SharedString, Window};
use std::cell::Cell;
use std::rc::Rc;

/// A command a feature offers: in ⌘K by its words, in a menu when it says which.
pub struct Action {
    /// A stable id (`updates.check`): a keymap or a setting may keep it.
    pub id: &'static str,
    /// What ⌘K lists ("Check for updates").
    pub words: &'static str,
    /// The menu that shows it, with its words there ("Check for Updates…").
    pub menu: Option<(Menu, &'static str)>,
    /// What runs it: the feature answers it in each tab (`Parts::actions`).
    pub act: Box<dyn gpui::Action>,
}

/// A menu a feature's command can go to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
    /// Artoo's own menu, above Settings….
    App,
}

/// What a part asks of the tab it was made for. Each call reaches the tab, so none is made while the shell is
/// calling the part (see the crate's doc).
pub trait Frame {
    /// A short word at the foot of the window.
    fn toast(&self, text: SharedString, cx: &mut App);

    /// The error line under the title bar.
    fn error(&self, text: SharedString, cx: &mut App);

    /// The tab drawn again: a part's card opened or closed by the part itself.
    fn redraw(&self, cx: &mut App);

    /// The tab's text selection, for text a part draws with the kit's markdown, so it is selected and copied
    /// as a comment's is; `None` once the tab is gone.
    fn selecting(&self, cx: &App) -> Option<Selecting>;
}

/// What a part does when the shell asks it (a card closed, polled, a snapshot view opened).
pub type Hook = Rc<dyn Fn(&mut App)>;

/// A key an open card takes.
pub type OnKey = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;

/// An item in the title bar, beside the fetch and the status.
pub struct TitleItem {
    pub view: AnyView,
    /// Whether it is shown now: one that is not takes no room.
    pub shown: Rc<dyn Fn(&App) -> bool>,
}

/// A card over the window. While it is open it is drawn above the tab and has the keys.
pub struct Overlay {
    pub view: AnyView,
    /// Whether it is open; the part sets it, the shell reads it.
    pub open: Rc<Cell<bool>>,
    /// A key while it is open (Escape included, should it close itself).
    pub key: OnKey,
    /// Closes it: Escape, when the card did not take it.
    pub close: Hook,
    /// Asked on each frame of a tab that is not busy (nothing being written, no card up): a card that opens on
    /// its own (a newer version found) opens here.
    pub poll: Option<Hook>,
}

/// A section of Settings a part can add rows to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    General,
}

/// What runs an action in a tab.
pub type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// What a feature draws in one tab. Every field is optional.
#[derive(Default)]
pub struct Parts {
    pub title: Option<TitleItem>,
    pub overlay: Option<Overlay>,
    /// Rows under a section of Settings, in the order given.
    pub settings: Vec<(Section, AnyView)>,
    /// The feature's actions as this tab runs them.
    pub actions: Vec<(Box<dyn gpui::Action>, Run)>,
    /// What `ARTOO_SNAPSHOT_VIEW=<name>` opens in this tab.
    pub snapshots: Vec<(&'static str, Hook)>,
}

type Maker = Rc<dyn Fn(Rc<dyn Frame>, &mut Window, &mut App) -> Parts>;

#[derive(Default)]
struct Registry {
    actions: Vec<Action>,
    makers: Vec<Maker>,
}

impl Global for Registry {}

/// Offers a command (a feature's `install`).
pub fn add_action(cx: &mut App, action: Action) {
    cx.default_global::<Registry>().actions.push(action);
}

/// The commands offered, in the order they were.
pub fn actions(cx: &App) -> &[Action] {
    cx.try_global::<Registry>()
        .map(|r| r.actions.as_slice())
        .unwrap_or(&[])
}

/// Adds what a feature draws in every tab (a feature's `install`): `make` runs for each tab, with its frame.
pub fn add_parts(
    cx: &mut App,
    make: impl Fn(Rc<dyn Frame>, &mut Window, &mut App) -> Parts + 'static,
) {
    cx.default_global::<Registry>().makers.push(Rc::new(make));
}

/// Every feature's parts for a new tab (the shell's).
pub fn make_parts(frame: Rc<dyn Frame>, window: &mut Window, cx: &mut App) -> Vec<Parts> {
    let makers = cx
        .try_global::<Registry>()
        .map(|r| r.makers.clone())
        .unwrap_or_default();
    makers
        .iter()
        .map(|make| make(frame.clone(), window, cx))
        .collect()
}
