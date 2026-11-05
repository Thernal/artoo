//! The shell's contract (#165 stage 4): what a feature adds to the window and asks of it, so a feature knows
//! this contract and never the shell, and the shell knows no feature.
//!
//! - **Actions** (`add_action`): a command's GPUI action, its words for ⌘K, the keys it ships with and where they
//!   apply, its item in the menu bar. ⌘K, the menu bar and Settings → Keyboard list what was offered, nothing
//!   else; the tab that has the keys runs one, or a feature through its parts (`Parts::actions`).
//! - **Parts** (`add_parts`): what a feature draws in every tab — an item in the title bar, a card over the
//!   window, a tool window on the right, rows in Settings, the views a snapshot run opens, a space's pages, the
//!   fields it types in (`Input`), its pieces of the item page on screen (`Piece`) —, made for each tab with its
//!   `Frame`.
//! - **Settings' sections** (`add_settings_section`): a section's id, title and place in the left column; the window
//!   root draws its own, a feature's holds its parts' rows (`Parts::settings`).
//! - **Editors** (`add_editor`): an editor Open in opens a file in at a line (`artoo_core::editor::Editor`), by
//!   its key, which Settings keeps; Artoo's three the frame's.
//! - **Pages**: the routes of some kinds and a space's own page, drawn by the feature that owns them. The shell
//!   keeps which is on screen — its history, `nav.txt` — and says so (`Pages::show`); a feature opens one of its
//!   own through the `Frame` like any other.
//! - **`Frame`**: what a part asks of its tab: a toast, the error line, a redraw, the text selection, the places
//!   it has open, its tool window opened or closed, a menu at the pointer, the narrow tab's drawer closed, a
//!   route opened (or a space's own page), a route kept among the places opened lately, the tab's services.
//! - **`Services`**: what the features of a tab offer one another — a contract's handle (`Rc<dyn Code>`), by its
//!   type —, set by the feature that makes it as its parts are made, read by the others when they use it.
//!
//! What a `Frame` changes waits until the update under way is over, so a part asks for it from anywhere. What it
//! reads needs the tab free: the shell calls a part's hooks (`key`, `close`, `poll`, `opened`, the snapshot views)
//! from inside its own update, where reading the tab would panic, so a hook defers what reads it (`cx.defer`).
//! The part's views, listeners and tasks read it as they like.
//!
//! - **Render contracts** (`render`): the pieces of the frame the tab draws — the page bar (`bar`), the title bar (`title`), the window's tabs (`tabs`), the body's row, zen's strips and the splitters (`frame`), the stripe and a tool window (`tools`), the spaces' menu (`spaces`), search everywhere's window (`palette`), Settings', Set up's and the welcome's frames (`settings`, `setup`, `welcome`), the keyboard card (`shortcuts`) — through a
//!   renderer the frame installs, so the tab knows this contract and never the frame's drawing.

pub mod bar;
pub mod frame;
pub mod palette;
pub mod render;
pub mod settings;
pub mod setup;
pub mod shortcuts;
pub mod spaces;
pub mod tabs;
pub mod title;
pub mod tools;
pub mod welcome;

use artoo_core::editor::Editor;
use artoo_core::git::WebRemote;
use artoo_core::route::Route;
use artoo_ui_kit::markdown::Selecting;
use gpui::{
    AnyElement, AnyView, App, Global, InteractiveElement, KeyDownEvent, MouseButton, Pixels, Point,
    Rgba, SharedString, Window,
};
use std::any::{Any, TypeId};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// A command: the GPUI action that runs it, and where the developer finds it — in ⌘K by its words, on the keys it
/// ships with, in a menu of the menu bar. ⌘K, the menu bar and Settings → Keyboard list only what was offered.
/// Made with `Action::new` and its builders.
pub struct Action {
    /// A stable id (`updates.check`), its subject's (`review.wrap`): a setting may keep it. A second action of the
    /// same id is refused.
    pub id: &'static str,
    /// What runs it: the tab answers it, or the feature in each tab (`Parts::actions`). `keymap.txt` keeps it by
    /// its GPUI name's last part (`CheckForUpdates`), so a second action of that name is refused too.
    pub act: Box<dyn gpui::Action>,
    /// What ⌘K lists ("Check for updates"); none, it is not there.
    pub words: Option<&'static str>,
    /// Where ⌘K lists it, the lowest first; those of one order as they were offered.
    pub order: u16,
    /// The keys it ships with, each where it applies; Settings → Keyboard changes them.
    pub keys: Vec<Key>,
    /// Its item in the menu bar.
    pub menu: Option<InMenu>,
}

/// Where ⌘K lists an action that does not say (`Action::order`): after Artoo's own.
pub const LATER: u16 = 1000;

impl Action {
    /// `act` under `id`, found nowhere yet: give it words, keys or a menu.
    pub fn new(id: &'static str, act: impl gpui::Action) -> Self {
        Self {
            id,
            act: Box::new(act),
            words: None,
            order: LATER,
            keys: Vec::new(),
            menu: None,
        }
    }

    /// In ⌘K by `words`.
    pub fn words(self, words: &'static str) -> Self {
        Self {
            words: Some(words),
            ..self
        }
    }

    /// Where ⌘K lists it (`Action::order`).
    pub fn order(self, order: u16) -> Self {
        Self { order, ..self }
    }

    /// Shipped on `keys` (macOS's way, `cmd-alt-/`; the app puts them in this OS's terms) where `when` applies
    /// (`when::TAB`, …).
    pub fn key(mut self, keys: &'static str, when: &'static str) -> Self {
        self.keys.push(Key { keys, when });
        self
    }

    /// In `menu` by `words` ("Check for Updates…"), in its first group, after the items that say where.
    pub fn menu(self, menu: Menu, words: &'static str) -> Self {
        Self {
            menu: Some(InMenu {
                menu,
                words,
                group: 0,
                order: LATER,
                checked: None,
            }),
            ..self
        }
    }

    /// Its menu item's place: in group `group` (a rule between groups, the lowest first), at `order` in it.
    pub fn at(mut self, group: u16, order: u16) -> Self {
        if let Some(item) = &mut self.menu {
            item.group = group;
            item.order = order;
        }
        self
    }

    /// Its menu item a choice, checked while `on` says so (asked as the menus are set).
    pub fn checked(mut self, on: impl Fn(&App) -> bool + 'static) -> Self {
        if let Some(item) = &mut self.menu {
            item.checked = Some(Rc::new(on));
        }
        self
    }
}

/// A key an action ships with (`Action::key`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Key {
    /// macOS's way: `cmd-alt-/`, `shift-n`, `cmd-k cmd-s`.
    pub keys: &'static str,
    /// The key context it applies in (`when`).
    pub when: &'static str,
}

/// Where a key applies (`Action::key`): a key context's predicate.
pub mod when {
    /// Anywhere in the app, with a window or not.
    pub const APP: &str = "";
    /// In a tab, typing or not.
    pub const TAB: &str = "Workspace";
    /// In a tab while nothing types: bare letters and arrows.
    pub const NOT_TYPING: &str = "Workspace && !Typing";
    /// In a tab while no field has a caret of its own (↑ ↓ move it there).
    pub const NO_CARET: &str = "Workspace && !Caret";
    /// In a tab while neither ⌘K nor a field with a caret is up.
    pub const LISTS: &str = "Workspace && !Palette && !Caret";
    /// In ⌘K.
    pub const PALETTE: &str = "Palette";
}

/// An action's item in the menu bar (`Action::menu`).
pub struct InMenu {
    pub menu: Menu,
    /// Its words there ("Check for Updates…").
    pub words: &'static str,
    /// Its group: a rule between groups, the lowest first.
    pub group: u16,
    /// Where it stands in its group, the lowest first; those of one order as they were offered.
    pub order: u16,
    /// A choice: whether it is on, asked as the menus are set.
    pub checked: Option<Checked>,
}

/// Whether a menu item that is a choice is on (`InMenu::checked`).
pub type Checked = Rc<dyn Fn(&App) -> bool>;

/// A menu of the menu bar, in its order there. A feature adds items to them; the menus themselves are the
/// frame's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Menu {
    /// Artoo's own menu: Settings… and Quit among it.
    App,
    File,
    Go,
    View,
    Help,
}

impl Menu {
    pub const ALL: [Menu; 5] = [Menu::App, Menu::File, Menu::Go, Menu::View, Menu::Help];

    /// Its title in the menu bar.
    pub fn title(self) -> &'static str {
        match self {
            Menu::App => "Artoo",
            Menu::File => "File",
            Menu::Go => "Go",
            Menu::View => "View",
            Menu::Help => "Help",
        }
    }
}

/// What a part asks of the tab it was made for. A change is made once the update under way is over; a read is
/// never made from a hook (see the crate's doc).
pub trait Frame {
    /// A toast at the foot of the window: news alone, or with a button (`Toast`).
    fn toast(&self, toast: Toast, cx: &mut App);

    /// The toasts whose button runs an action `over` picks go now: news another has taken in.
    fn drop_toasts(&self, over: Pick, cx: &mut App);

    /// The error line under the title bar.
    fn error(&self, text: SharedString, cx: &mut App);

    /// The tab drawn again: a part's card opened or closed by the part itself.
    fn redraw(&self, cx: &mut App);

    /// The tab's text selection, for text a part draws with the kit's markdown, so it is selected and copied
    /// as a comment's is; `None` once the tab is gone.
    fn selecting(&self, cx: &App) -> Option<Selecting>;

    /// What the tab has open, and had.
    fn places(&self, cx: &App) -> Places;

    /// Opens a part's tool window by its id, or gives the open one the keyboard; `None` closes the one open.
    fn show_tool(&self, id: Option<&'static str>, cx: &mut App);

    /// Opens a part's tool window by its id, or closes it when it is the one open: the key that shows or hides it.
    fn toggle_tool(&self, id: &'static str, cx: &mut App);

    /// Whether tool window `id` has the keyboard: open, its zone the focused one, nothing over it, typing.
    fn tool_typing(&self, id: &str, cx: &App) -> bool;

    /// A part's input given the keyboard by its id (`Input::id`): a press in its field.
    fn take_keyboard(&self, id: &'static str, cx: &mut App);

    /// A part's input lets the keyboard go while it has it: a press outside its field.
    fn leave_keyboard(&self, id: &'static str, cx: &mut App);

    /// A menu at `at`, a rule between its groups; an item picked runs once the menu has closed.
    fn menu(&self, at: Point<Pixels>, groups: Vec<Vec<MenuItem>>, cx: &mut App);

    /// The side panel's drawer closed, when the tab is narrow and it is open.
    fn close_drawer(&self, cx: &mut App);

    /// Route `route` opened: on screen in its space, kept in the history, shown by the pages of its kind. One no
    /// pages draw says so in a toast.
    fn open(&self, route: Route, cx: &mut App);

    /// Space `space`'s own page opened (`code`: the project's).
    fn open_space(&self, space: &'static str, cx: &mut App);

    /// Route `route` kept first among the places opened lately (⌘K lists them).
    fn remember(&self, route: Route, cx: &mut App);

    /// What the tab's features offer one another.
    fn services(&self) -> Rc<Services>;

    /// A space's own page as a list beside a preview, in the tab's frame: the list at the width the developer
    /// gave it, bordered while it has the keyboard, its handle, the preview beside; narrow, the list alone. Asked
    /// from a page's own drawing, never from inside the tab's update.
    fn list_page(&self, list: AnyElement, preview: AnyElement, cx: &mut App) -> AnyElement;

    /// What is typed in the page bar's filter of a space's list (⌘F).
    fn filter(&self, cx: &App) -> String;

    /// The width of a space's list, as the developer gave it (`list_page`), for a page drawn as one while it
    /// waits.
    fn list_width(&self, cx: &App) -> f32;

    /// The handle beside a space's list (`list_page`), for a page that draws its list itself: it widens or
    /// narrows the list, a double click setting it back. Asked from a page's own drawing.
    fn list_handle(&self, cx: &mut App) -> AnyElement;

    /// What the route on screen is of, a part's piece (`Slot::Identity`), folding away in zen; none while no diff
    /// is shown. Asked from a page's own drawing.
    fn identity(&self, cx: &mut App) -> Option<AnyElement>;

    /// The parts' pieces in `slot` for the route on screen, in their order. Asked from a page's own drawing.
    fn pieces(&self, slot: Slot, cx: &mut App) -> Vec<AnyElement>;

    /// The first part's piece in `slot` for the route on screen. Asked from a page's own drawing.
    fn piece(&self, slot: Slot, cx: &mut App) -> Option<AnyElement>;

    /// What the shell puts on the diff's column. Asked from a page's own drawing.
    fn chrome(&self, cx: &mut App) -> Chrome;

    /// The side panel's list with the keyboard, if one has it, and the mark its title wears then (` · ⌃2`). Asked
    /// from a view's own drawing.
    fn list_zone(&self, cx: &mut App) -> Option<(ListZone, SharedString)>;

    /// The part's input with the keyboard, by its id (`Input::id`). Asked from a view's own drawing.
    fn typing(&self, cx: &mut App) -> Option<&'static str>;

    /// The route on screen; `None` on a space's own page. Asked from a view's own drawing.
    fn route(&self, cx: &App) -> Option<Route>;

    /// The item page's section on screen when it is a page of its own; `None` on its changes. Asked from a view's
    /// own drawing.
    fn item_tab(&self, cx: &App) -> Option<ItemTab>;
}

/// An item page's section that is a page of its own (#73, #74); its changes are the review's item page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemTab {
    /// A request's Overview.
    Overview,
    /// A request's or a branch's Commits.
    Commits,
}

/// A list of the side panel the keyboard can be in, each with its keys (⌃2, ⌃3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListZone {
    /// The commits: a branch's, or a comparison's own.
    Commits,
    /// The changed files.
    Files,
}

/// What the shell puts on an item page's diff column (`Frame::chrome`).
pub struct Chrome {
    /// The column's border: whether the keyboard works in the diff.
    pub border: Rgba,
    /// A menu is open (View ▾'s among them).
    pub menu_open: bool,
    /// The app the diff's file opens in, ▾ the others.
    pub open_in: Option<AnyElement>,
    /// The file as a page, where the review shows it so, with the tab's pins on it.
    pub alone: Option<AnyElement>,
    /// Zen shows one file at a time.
    pub zen_one: bool,
    /// The part's input with the keyboard, by its id (`Input::id`).
    pub typing: Option<&'static str>,
}

/// What the features of one tab offer one another, by type: a contract's handle, `Rc<dyn …>`.
#[derive(Default)]
pub struct Services(RefCell<HashMap<TypeId, Box<dyn Any>>>);

impl Services {
    /// `handle` offered, in place of the one of its type there was.
    pub fn set<T: Clone + 'static>(&self, handle: T) {
        self.0
            .borrow_mut()
            .insert(TypeId::of::<T>(), Box::new(handle));
    }

    /// The handle of type `T`, when a feature offers one.
    pub fn get<T: Clone + 'static>(&self) -> Option<T> {
        self.0
            .borrow()
            .get(&TypeId::of::<T>())?
            .downcast_ref()
            .cloned()
    }
}

/// News at the foot of the window, raised through the `Frame` by the feature it is about, which says what it
/// reads and when it gives way; the tab only shows it. Its button runs an action — its id and its fields, as a
/// route is a kind and its fields — that the tab answers: the tab's own, or a part's (`Parts::actions`; one
/// without fields, as `Run` is not handed the action).
pub struct Toast {
    /// What it says of `count` things.
    pub say: Rc<dyn Fn(usize) -> String>,
    /// How many things it tells of: a toast whose button runs the same action, still showing, goes, its count
    /// added to this one's.
    pub count: usize,
    /// Its button: its words ("Review") and the action it runs.
    pub button: Option<(&'static str, Box<dyn gpui::Action>)>,
    /// With a button, the tag it also goes to the system's notifications under while no window of Artoo's is in
    /// front, the newer replacing the older; a click there runs the button.
    pub system: Option<String>,
}

/// Which toasts go (`Frame::drop_toasts`): those whose button runs an action it picks.
pub type Pick = Box<dyn Fn(&dyn gpui::Action) -> bool>;

impl Toast {
    /// News alone.
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            say: Rc::new(move |_| text.clone()),
            count: 1,
            button: None,
            system: None,
        }
    }

    /// With a button saying `words`, running `act`.
    pub fn button(mut self, words: &'static str, act: impl gpui::Action) -> Self {
        self.button = Some((words, Box::new(act)));
        self
    }

    /// About `count` things, `say` wording however many it ends up telling of.
    pub fn counted(mut self, count: usize, say: impl Fn(usize) -> String + 'static) -> Self {
        self.count = count;
        self.say = Rc::new(say);
        self
    }

    /// To the system's notifications as well, under `tag`.
    pub fn system(mut self, tag: impl Into<String>) -> Self {
        self.system = Some(tag.into());
        self
    }
}

impl From<&str> for Toast {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for Toast {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

/// An item of a menu a part opens (`Frame::menu`).
pub struct MenuItem {
    pub label: String,
    /// Its keys, said at its end (`⌘W`).
    pub key: &'static str,
    /// `Some` for a choice: whether it is on.
    pub checked: Option<bool>,
    /// What it does; `None` for one shown dimmed, its label saying why.
    pub run: Option<Run>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, run: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        Self {
            label: label.into(),
            key: "",
            checked: None,
            run: Some(Rc::new(run)),
        }
    }

    /// One that cannot be picked here, its label saying why (`Close — pinned`).
    pub fn off(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            key: "",
            checked: None,
            run: None,
        }
    }

    pub fn key(self, key: &'static str) -> Self {
        Self { key, ..self }
    }

    pub fn checked(self, on: bool) -> Self {
        Self {
            checked: Some(on),
            ..self
        }
    }
}

/// What a tab has open, and had: what a part names it by, or leaves out of what it sends.
#[derive(Clone, Default)]
pub struct Places {
    /// The repository open in it.
    pub root: Option<PathBuf>,
    /// The project's main working tree, whatever worktree of it is open.
    pub project: Option<PathBuf>,
    /// The repositories opened before.
    pub recent: Vec<PathBuf>,
    /// The repository's address on the web.
    pub web: Option<WebRemote>,
    /// The host `web` is on, without the scheme.
    pub host: Option<String>,
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

/// A tool window on the right: its icon in the stripe of the spaces it is in, its window beside the page once
/// opened. Typed keys go to it while it has the keyboard and says it is typing.
pub struct Tool {
    /// A stable id (`report`): the layout keeps the window open by it.
    pub id: &'static str,
    /// The stripe's icon (`flag`).
    pub icon: &'static str,
    /// The icon's long hover ("Report a problem  ⌥⌘/").
    pub tip: &'static str,
    /// The window's title; the shell draws it with the close button.
    pub title: &'static str,
    /// The spaces it is in, by their names (`code`, `branches`, `requests`, `inbox`); none, every one.
    pub spaces: &'static [&'static str],
    /// Only while a page is on screen, not on its space's own page (a request's Discussions).
    pub on_pages: bool,
    /// Where its icon stands in the stripe: the lowest first, those of one order as their features were
    /// installed. What is about the page (Outline, Discussions) comes before what is in every space.
    pub order: u16,
    /// What its icon in the stripe says, asked as the stripe is drawn.
    pub badge: Option<OnBadge>,
    /// What is said after the title, asked as the window is drawn; none, its badge's words while it counts.
    pub count: Option<OnCount>,
    /// What is under the title: a body that scrolls itself and its foot, if it has one (the kit's
    /// `controls::tool_foot`).
    pub view: AnyView,
    /// As it opens, before its window is drawn: a picture of the window taken now does not show it.
    pub opened: Hook,
    /// Whether it takes typed keys while it has the keyboard; the part sets it, the shell reads it.
    pub typing: Rc<Cell<bool>>,
    /// A typed key while it has the keyboard.
    pub key: OnKey,
    /// A window that lists: ↑ ↓ ↵ while it has the keyboard, ⌫ while it is open, as the shell's lists take them.
    pub list_key: Option<OnListKey>,
}

/// What a tool window's icon says, asked as the stripe is drawn: while the tab draws, so it reads its own state and
/// never the tab (through a host or otherwise).
pub type OnBadge = Rc<dyn Fn(&App) -> Badge>;

/// What a tool window says after its title (`Tool::count`), asked as the window is drawn: while the tab draws, so
/// it reads its own state and never the tab.
pub type OnCount = Rc<dyn Fn(&App) -> Option<String>>;

/// What a tool window's icon in the stripe says (`Tool::badge`).
pub struct Badge {
    /// A count on the icon, in `color`; none at 0.
    pub count: usize,
    pub color: Rgba,
    /// What it counts, in the icon's long hover ("3 drafts").
    pub what: String,
    /// Drawn under the icon (the agents': a dot for each chat listening).
    pub under: Option<AnyElement>,
}

/// A key the shell's lists answer, given to a tool window that lists (`Tool::list_key`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ListKey {
    Up,
    Down,
    /// ↵: the row picked acted on.
    Enter,
    /// ⌫: the row picked deleted.
    Delete,
}

/// A key a tool window that lists takes; whether it took it.
pub type OnListKey = Rc<dyn Fn(ListKey, &mut App) -> bool>;

/// A field a part draws that takes typed keys (#168). One input at a time has the keyboard, a part's as the tab's
/// own: one opened since the last look takes it, and so does a press in its box; a press outside it, another page
/// or esc lets it go, its text kept.
pub struct Input {
    /// A stable id (`branches.search`).
    pub id: &'static str,
    /// Whether it is open, with the keyboard or not.
    pub open: Rc<dyn Fn(&App) -> bool>,
    /// A key while it has the keyboard, and what came of it. The shell calls it from inside its own update.
    pub key: OnType,
    /// It let the keyboard go: one left empty closes.
    pub left: Hook,
    /// The text selected in it, for Copy.
    pub selected: Selected,
    /// ↑ ↓ are its own while it has the keyboard: they move its caret or what it points at.
    pub caret: Rc<dyn Fn(&App) -> bool>,
}

/// `el` as the box of a part's input `id` (`Input::id`): a press in it gives the input the keyboard, one outside
/// takes it away.
pub fn input_box<E: InteractiveElement>(el: E, id: &'static str, frame: &Rc<dyn Frame>) -> E {
    let (take, leave) = (frame.clone(), frame.clone());
    el.on_mouse_down(MouseButton::Left, move |_, _, cx| {
        take.take_keyboard(id, cx)
    })
    .on_mouse_down_out(move |_, _, cx| leave.leave_keyboard(id, cx))
}

/// A key typed in a part's input (`Input::key`).
pub type OnType = Rc<dyn Fn(&KeyDownEvent, &mut Window, &mut App) -> Typed>;

/// The text selected in a part's input (`Input::selected`).
pub type Selected = Rc<dyn Fn(&App) -> Option<String>>;

/// What a key typed in a part's input came to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Typed {
    /// Taken, or not one it takes: it keeps the keyboard.
    Kept,
    /// Esc: it lets the keyboard go.
    Leave,
}

/// Where a part's piece of an item page goes (`Piece`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Slot {
    /// Over the page, saying what it is of: the first part's that has one for the route.
    Identity,
    /// At the top of the diff's column (the file's pane, or what stands in the diff's place), over what the shell
    /// puts there: every part's that has one, in their order.
    DiffTop,
    /// On the left of the diff's toolbar, saying which range is shown: the first part's that has one.
    DiffBar,
    /// Over the diff's column, under the identity: every part's that has one, in their order.
    OverDiff,
    /// Over the diff's rows, painted last: every part's that has one, in their order.
    OverRows,
    /// In the diff's place, instead of the file's pane: the first part's that has one.
    Instead,
}

/// A piece of the item page on screen a part draws in a slot (#165 I5c): for the route on screen, or nothing. The
/// shell calls it from inside its own update, as it does a hook, so what it would ask the `Frame` it is handed.
pub struct Piece {
    /// A stable id (`branches.line`).
    pub id: &'static str,
    pub slot: Slot,
    pub draw: Draw,
}

/// What a piece draws for a route (`Piece::draw`).
pub type Draw = Rc<dyn Fn(&Route, &Drawing, &mut App) -> Option<AnyElement>>;

/// What the shell hands a piece as it draws it: what the piece cannot ask the `Frame` while it is drawn.
pub struct Drawing {
    /// The tab's text selection (`Frame::selecting`).
    pub selecting: Selecting,
    /// The repository's address on the web (`Places::web`).
    pub web: Option<WebRemote>,
    /// The filter of the list on screen (`Frame::filter`).
    pub filter: String,
    /// The part's input with the keyboard, by its id (`Input::id`).
    pub typing: Option<&'static str>,
}

/// What runs an action in a tab.
pub type Run = Rc<dyn Fn(&mut Window, &mut App)>;

/// A space's pages a feature draws in one tab: the routes of its kinds, and the space's own page when none of
/// them is on screen.
pub struct Pages {
    /// The space they are in, by its id (`code`).
    pub space: &'static str,
    /// The kinds of route they draw (`code.file`).
    pub kinds: &'static [&'static str],
    /// The page on screen.
    pub view: AnyView,
    /// The panel beside it (Code's tree, an item's side panel) for the route on screen, at the width given; the
    /// shell puts it where the side panel goes, over the page in a drawer when the tab is narrow. None beside an
    /// item's section that is a page of its own (`ItemTab`).
    pub side: Option<Side>,
    /// What is on screen now: a route of theirs, or the space's own page (`None`); `true` when it was just opened
    /// (a click, a link), so what it shows is read anew, `false` when it was gone back to or kept from before.
    /// The shell says it from inside its own update (see the crate's doc).
    pub show: Show,
}

/// A panel beside a space's pages for the route on screen (`None`: the space's own page), drawn at a width; none
/// where the space has none there.
pub type Side = Rc<dyn Fn(Option<&Route>, f32, &mut App) -> Option<AnyElement>>;

/// What a space's pages are told is on screen (`Pages::show`).
pub type Show = Rc<dyn Fn(Option<&Route>, bool, &mut App)>;

/// What a feature draws in one tab. Every field is optional.
#[derive(Default)]
pub struct Parts {
    pub title: Option<TitleItem>,
    pub overlay: Option<Overlay>,
    pub tool: Option<Tool>,
    /// Rows under a section of Settings, by its id (`settings::Section::id`), in the order given.
    pub settings: Vec<(&'static str, AnyView)>,
    /// The feature's actions as this tab runs them.
    pub actions: Vec<(Box<dyn gpui::Action>, Run)>,
    /// What `ARTOO_SNAPSHOT_VIEW=<name>` opens in this tab.
    pub snapshots: Vec<(&'static str, Hook)>,
    /// A space's pages.
    pub pages: Option<Pages>,
    /// The fields it types in.
    pub inputs: Vec<Input>,
    /// Its pieces of the item page on screen.
    pub pieces: Vec<Piece>,
}

type Maker = Rc<dyn Fn(Rc<dyn Frame>, &mut Window, &mut App) -> Parts>;

#[derive(Default)]
struct Registry {
    actions: Vec<Action>,
    makers: Vec<Maker>,
    editors: Vec<Editor>,
    sections: Vec<settings::Section>,
    /// What was offered a second time and refused: `action review.wrap (its id)`.
    refused: Vec<String>,
}

impl Global for Registry {}

/// Offers an editor to open files in (a feature's `install`). A key already offered is refused, and the log
/// says so: the first to offer it keeps it.
pub fn add_editor(cx: &mut App, editor: Editor) {
    let registry = cx.default_global::<Registry>();
    if registry.editors.iter().any(|e| e.key == editor.key) {
        refuse(registry, format!("editor {}", editor.key));
        return;
    }
    registry.editors.push(editor);
}

/// The editors offered, in the order they were.
pub fn editors(cx: &App) -> &[Editor] {
    cx.try_global::<Registry>()
        .map(|r| r.editors.as_slice())
        .unwrap_or(&[])
}

/// Offers a section of Settings (a feature's `install`). An id offered already is refused, and the log says so.
pub fn add_settings_section(cx: &mut App, section: settings::Section) {
    let registry = cx.default_global::<Registry>();
    if registry.sections.iter().any(|s| s.id == section.id) {
        refuse(registry, format!("settings section {}", section.id));
        return;
    }
    registry.sections.push(section);
}

/// The sections of Settings offered, in their order.
pub fn settings_sections(cx: &App) -> Vec<settings::Section> {
    let mut sections = cx
        .try_global::<Registry>()
        .map(|r| r.sections.clone())
        .unwrap_or_default();
    sections.sort_by_key(|s| s.order);
    sections
}

/// Offers a command (a feature's `install`). One whose id, or whose GPUI name's last part (what `keymap.txt` keeps),
/// was offered already is refused, and the log says so: the first to offer it keeps it.
pub fn add_action(cx: &mut App, action: Action) {
    let registry = cx.default_global::<Registry>();
    if let Some(taken) = taken(&registry.actions, &action) {
        refuse(registry, format!("action {} ({taken})", action.id));
        return;
    }
    registry.actions.push(action);
}

/// `what` refused, kept and logged.
fn refuse(registry: &mut Registry, what: String) {
    artoo_core::applog::line(&format!("{what} offered twice: the second refused"));
    registry.refused.push(what);
}

/// What was offered a second time and refused (the contributions check: none).
pub fn refused(cx: &App) -> Vec<String> {
    cx.try_global::<Registry>()
        .map(|r| r.refused.clone())
        .unwrap_or_default()
}

/// What of `action`'s a command offered before has: its id or its GPUI name's last part.
fn taken(actions: &[Action], action: &Action) -> Option<&'static str> {
    let name = short(action.act.name());
    actions.iter().find_map(|a| {
        if a.id == action.id {
            Some("its id")
        } else if short(a.act.name()) == name {
            Some("its name")
        } else {
            None
        }
    })
}

/// `artoo::NextFile` → `NextFile`: what `keymap.txt` names an action by.
pub fn short(name: &str) -> &str {
    name.rsplit("::").next().unwrap_or(name)
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

#[cfg(test)]
mod tests {
    use super::*;

    trait Greeting {
        fn hello(&self) -> &'static str;
    }

    struct En;

    impl Greeting for En {
        fn hello(&self) -> &'static str {
            "hello"
        }
    }

    gpui::actions!(test, [Wrap, Unwrap]);
    mod other {
        gpui::actions!(elsewhere, [Wrap]);
    }

    #[test]
    fn an_action_offered_twice_is_refused() {
        let wrap = Action::new("review.wrap", Wrap);
        let actions = vec![wrap];
        // Its id again, under another action.
        assert_eq!(
            taken(&actions, &Action::new("review.wrap", Unwrap)),
            Some("its id")
        );
        // Its name in another namespace: `keymap.txt` could not tell them apart.
        assert_eq!(
            taken(&actions, &Action::new("other.wrap", other::Wrap)),
            Some("its name")
        );
        assert_eq!(taken(&actions, &Action::new("review.unwrap", Unwrap)), None);
    }

    #[test]
    fn a_menu_place_is_set_on_its_item() {
        let action = Action::new("review.wrap", Wrap)
            .menu(Menu::View, "Wrap Long Lines")
            .at(3, 1)
            .checked(|_| true);
        let item = action.menu.unwrap();
        assert_eq!((item.menu, item.group, item.order), (Menu::View, 3, 1));
        assert!(item.checked.is_some());
        // Without a menu, a place has nowhere to go.
        assert!(Action::new("review.wrap", Wrap).at(3, 1).menu.is_none());
    }

    #[test]
    fn a_service_is_found_by_its_handles_type() {
        let services = Services::default();
        assert!(services.get::<Rc<dyn Greeting>>().is_none());
        services.set::<Rc<dyn Greeting>>(Rc::new(En));
        let greeting = services.get::<Rc<dyn Greeting>>().unwrap();
        assert_eq!(greeting.hello(), "hello");
        assert!(services.get::<Rc<En>>().is_none());
    }
}
