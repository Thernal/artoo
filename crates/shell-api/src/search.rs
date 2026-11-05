//! Search everywhere's scopes (⌘K's tabs, #52) a feature offers in a tab (`Parts::searches`): its words, the action
//! that opens it, what it finds for a text and where each find goes — a route opened as the tab opens one
//! (`Frame::open`), or for what is no place the owner's own doing. The tab keeps the window, its field and the
//! scopes that are its own (All, Actions), and puts each offered one among them by its `order`.
//!
//! The same window shows a list of its own alone (`Parts::lists`, `List`): the themes to try, a comparison's refs —
//! what it says as it stands, its finds, what it does as it opens, as the selection comes to a find, as it closes.

use artoo_core::route::Route;
use gpui::{App, SharedString};
use std::rc::Rc;

/// A scope of search everywhere as its owner offers it in a tab.
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
    /// What it reads once as ⌘K opens, for the finds that follow (the project's files); None when nothing. Run at
    /// once, in the tab's update: it reads its owner's state, never the tab.
    pub opened: Option<Doing>,
}

/// A list the window shows alone, opened by its own action (`Frame::search` by its id).
pub struct List {
    /// Its id (`branches.compare`).
    pub id: &'static str,
    /// What it says as it stands: its title, the field's placeholder, its line when it finds nothing.
    pub says: Says,
    /// What it finds for the text (`Ask::all` false), the best first.
    pub find: Finds,
    /// What it reads once as it opens, for the finds that follow (a repository's refs). Run at once, in the tab's
    /// update: it reads its owner's state, never the tab.
    pub opened: Option<Doing>,
    /// Closed without a pick (esc, a click outside, its key again): what was tried put back. Run at once, as
    /// `opened` is.
    pub closed: Option<Doing>,
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
/// `order` here; or, the text starting with `prefix` (`#`), only its own — 14 of them, before more is typed too.
#[derive(Clone, Copy, Debug)]
pub struct AllOf {
    pub few: usize,
    pub order: u16,
    pub prefix: Option<char>,
}

/// What a scope is asked: the text typed (without All's prefix), and whether for the All tab.
#[derive(Clone, Copy, Debug)]
pub struct Ask<'a> {
    pub text: &'a str,
    pub all: bool,
}

/// What a scope finds for a text. Called while the tab draws and as it types: it reads the feature's own state,
/// never the tab.
pub type Finds = Rc<dyn Fn(Ask, &App) -> Vec<Find>>;

/// A find: the group it is listed under, its words, and where a pick goes; what else its row is (`Find::new`, then
/// what applies).
pub struct Find {
    pub group: &'static str,
    pub label: String,
    pub detail: String,
    /// The file it is, by its path: its row has the file's icon, and its words are a file's name and folder.
    pub file: Option<String>,
    /// A face, its row drawn in it (#24).
    pub face: Option<SharedString>,
    /// The one in force (the theme on the window): its list opens on it.
    pub current: bool,
    /// What it tries as the selection comes to it (a theme on the whole window), its list's `closed` putting it
    /// back. Run at once, in the tab's update: it changes its owner's state, never the tab's.
    pub tried: Option<Doing>,
    pub open: Opens,
}

impl Find {
    pub fn new(group: &'static str, label: String, detail: String, open: Opens) -> Self {
        Self {
            group,
            label,
            detail,
            file: None,
            face: None,
            current: false,
            tried: None,
            open,
        }
    }

    pub fn file(mut self, path: String) -> Self {
        self.file = Some(path);
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
    /// revision the diff shows, a comparison's second list —, the owner's doing: run once ⌘K has closed, outside
    /// the tab's update.
    Run(Doing),
}

/// Something a scope's owner does.
pub type Doing = Rc<dyn Fn(&mut App)>;
