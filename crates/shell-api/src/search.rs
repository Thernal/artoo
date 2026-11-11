//! Search everywhere's scopes (⌘K's tabs, #52) a feature offers in a tab (`Parts::searches`): its words, the action
//! that opens it, what it finds for a text and where each find goes — a route opened as the tab opens one
//! (`Frame::open`), or for what is no place the owner's own doing. The window is the shell's, one in each tab
//! (`SearchWindow`, in the tab's services): it has the scopes that are its own (All, Actions) and puts each offered one
//! among them by its `order`; the tab hands it what its parts offer, gives it the keyboard while it is open, draws it.
//!
//! The same window shows a list of its own alone (`Parts::lists`, `List`): the themes to try, a comparison's refs, the
//! tab's places opened lately — what it says as it stands, its finds, what it does as it opens, as the selection
//! comes to a find, as ⇥ is pressed, as it closes.
//!
//! What a scope or a list finds and does runs outside the tab's update, so it may read the tab (the tab's own lists
//! read its places); what it says is read as the window draws, and reads its owner's state alone.

use artoo_core::route::Route;
use gpui::{AnyView, App, KeyDownEvent, SharedString, Window};
use std::rc::Rc;

/// The All tab's id, the window's own (`SearchWindow::search`).
pub const ALL: &str = "shell.all";
/// The Actions tab's id, the window's own: every command offered with words.
pub const ACTIONS: &str = "shell.actions";
/// The list of the places opened lately, the tab's: empty, ⌘K's All shows its first under `Here` (#192).
pub const RECENT: &str = "recent";

/// ⌘K's window in a tab, the shell's (`Services`). It never asks the tab while it is changed: what a scope or a list
/// finds and does runs once no entity is.
pub trait SearchWindow {
    /// The scopes and the lists the tab's parts offer, the tab's own among them: handed once the parts are made.
    fn offer(&self, searches: Vec<Search>, lists: Vec<List>, cx: &mut App);
    /// Opened on scope `id` (`ALL`, `ACTIONS`, an offered one's), or moved to its tab while open, or closed when it is
    /// the tab open — its key again; or opened on list `id` alone. What it opening clears in the tab, the tab clears.
    fn search(&self, id: &'static str, cx: &mut App);
    /// Closed without a pick, what a list tried put back.
    fn close(&self, cx: &mut App);
    fn is_open(&self, cx: &App) -> bool;
    /// A key while it has the keyboard: the tab's first input while it is open.
    fn key(&self, event: &KeyDownEvent, window: &mut Window, cx: &mut App);
    /// ↑ ↓: the selection to the find before or after.
    fn step(&self, down: bool, cx: &mut App);
    /// The text selected in its field, for Copy.
    fn selected(&self, cx: &App) -> Option<String>;
    /// What the tab draws over itself while it is open.
    fn view(&self, cx: &App) -> Option<AnyView>;
    /// Its field's text set, as typing it would (UI checks).
    fn type_text(&self, text: &str, cx: &mut App);
}

/// A scope of search everywhere as its owner offers it in a tab.
#[derive(Clone)]
pub struct Search {
    /// Its id (`requests`), by which the action that opens it names it.
    pub id: &'static str,
    /// Its tab's name (`Requests`), with what it found after it.
    pub label: &'static str,
    /// What the field says while nothing is typed.
    pub placeholder: &'static str,
    /// The action that opens ⌘K on it, by its id (`shell.search-requests`): the foot says its key in force.
    pub go: &'static str,
    /// Its place among ⌘K's tabs, the lowest first: All 0, Files 10, Commits 20, Actions 90.
    pub order: u16,
    /// What the All tab shows of it.
    pub all: AllOf,
    /// What it finds for the text, the best first; for the All tab (`Ask::all`) as All matches.
    pub find: Finds,
    /// What it reads once as ⌘K opens, for the finds that follow (the project's files); None when nothing.
    pub opened: Option<Doing>,
}

/// A list the window shows alone, opened by its own action (`Frame::search` by its id).
#[derive(Clone)]
pub struct List {
    /// Its id (`branches.compare`).
    pub id: &'static str,
    /// What it says as it stands: its title, the field's placeholder, its line when it finds nothing. Read as the
    /// window draws: it reads its owner's state, never the tab.
    pub says: Says,
    /// What it finds for the text (`Ask::all` false), the best first.
    pub find: Finds,
    /// What it reads once as it opens, for the finds that follow (a repository's refs, the faces installed).
    pub opened: Option<Doing>,
    /// Closed without a pick (esc, a click outside, its key again): what was tried put back.
    pub closed: Option<Doing>,
    /// What ⇥ does in it (Switch to: only what is not reviewed yet, or all); it is found again from the first.
    pub tab: Option<Doing>,
    /// The list esc goes back to, the text cleared, instead of closing (a comparison's second pick, to its first).
    pub back: Option<&'static str>,
}

/// What a list says now.
pub type Says = Rc<dyn Fn(&App) -> Said>;

/// A list's words: a title over the field, the field's placeholder, the line when nothing is found.
pub struct Said {
    pub title: Option<String>,
    pub placeholder: &'static str,
    pub empty: &'static str,
}

/// What the All tab shows of a scope: once something is typed, its first `few` finds, after the scopes of a lower
/// `order` here; or, the text starting with `prefix` (`#`), only its own — 14 of them, before more is typed too. One
/// `alone` is All's only, with no tab of its own: what it finds, All shows alone (a pasted link, one place to go).
#[derive(Clone, Copy, Debug)]
pub struct AllOf {
    pub few: usize,
    pub order: u16,
    pub prefix: Option<char>,
    pub alone: bool,
}

/// What a scope is asked: the text typed (without All's prefix), and whether for the All tab.
#[derive(Clone, Copy, Debug)]
pub struct Ask<'a> {
    pub text: &'a str,
    pub all: bool,
}

/// What a scope finds for a text, as the window opens and as it is typed in.
pub type Finds = Rc<dyn Fn(Ask, &App) -> Vec<Find>>;

/// A find: the group it is listed under, its words, and where a pick goes; what else its row is (`Find::new`, then
/// what applies).
pub struct Find {
    pub group: SharedString,
    pub label: String,
    pub detail: String,
    /// The key that picks it, as the row says it at its end (`↵`, Browse…'s `⌘O`); empty for none.
    pub key: String,
    /// The file it is, by its path: its row has the file's icon, and its words are a file's name and folder.
    pub file: Option<String>,
    /// Its words a name and the folder it is in, as a file's are, without a file's icon (a repository).
    pub in_folder: bool,
    /// A face, its row drawn in it (#24).
    pub face: Option<SharedString>,
    /// The one in force (the theme on the window): its list opens on it.
    pub current: bool,
    /// What it tries as the selection comes to it (a theme on the whole window), its list's `closed` putting it
    /// back.
    pub tried: Option<Doing>,
    pub open: Opens,
}

impl Find {
    pub fn new(group: impl Into<SharedString>, label: String, detail: String, open: Opens) -> Self {
        Self {
            group: group.into(),
            label,
            detail,
            key: String::new(),
            file: None,
            in_folder: false,
            face: None,
            current: false,
            tried: None,
            open,
        }
    }

    pub fn key(mut self, key: String) -> Self {
        self.key = key;
        self
    }

    pub fn file(mut self, path: String) -> Self {
        self.file = Some(path);
        self
    }

    pub fn in_folder(mut self) -> Self {
        self.in_folder = true;
        self
    }

    pub fn face(mut self, face: SharedString) -> Self {
        self.face = Some(face);
        self
    }

    pub fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    pub fn tried(mut self, tried: Doing) -> Self {
        self.tried = Some(tried);
        self
    }
}

/// Where a pick goes.
#[derive(Clone)]
pub enum Opens {
    /// A place, opened as the tab opens one (`Frame::open`).
    Route(Route),
    /// What is no place, or more than going to one — a file of the diff on screen picked there, a file read at the
    /// revision the diff shows, a comparison's second list —, the owner's doing, run once ⌘K has closed.
    Run(Doing),
}

/// Something a scope's owner does.
pub type Doing = Rc<dyn Fn(&mut App)>;
