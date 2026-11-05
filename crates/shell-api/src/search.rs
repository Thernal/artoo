//! Search everywhere's scopes (⌘K's tabs, #52) a feature offers in a tab (`Parts::searches`): its words, the action
//! that opens it, what it finds for a text and where each find goes, a route opened as the tab opens one
//! (`Frame::open`). The tab keeps the window, its field and the scopes that are its own (All, Actions; Files and
//! Commits until their owners take them, #171 7b), and puts each offered one among them by its `order`.

use artoo_core::route::Route;
use gpui::App;
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

/// A find: the group it is listed under, its words, and where a pick goes.
pub struct Find {
    pub group: &'static str,
    pub label: String,
    pub detail: String,
    pub open: Route,
}
