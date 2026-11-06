//! Set up's contract (#143 B): a page of tabs, each a part — its Settings section at its shortest — with a check on a
//! part done, the part in the column between, the count and Back · Skip · Next. The parts are their owners'
//! (`Parts::setup`: Agents the agents', Keys and Look the tab's); the page, which parts are offered and when — once
//! each (`setup.txt`), a release that adds one that one alone, every part from Help → Set Up Artoo… — is the shell's,
//! one per tab behind `SetUp` in its services.

use crate::Hook;
use gpui::{AnyElement, AnyView, App, Window};
use std::rc::Rc;

/// A part of Set up (`Parts::setup`).
#[derive(Clone)]
pub struct Part {
    /// Its key in `setup.txt` (`agents`), which keeps the parts offered once, and in a snapshot's view
    /// (`setup-keys`).
    pub key: &'static str,
    /// Its tab's words.
    pub name: &'static str,
    /// Its place among the tabs.
    pub order: u32,
    /// Whether it is done as its Settings section says: its tab's check.
    pub done: Asks,
    /// What it holds while it is the part on. Drawn by the page's own view, outside the tab's update, so it may read
    /// and draw the tab.
    pub body: Body,
    /// It came on screen (the page opened on it, its tab picked): what it shows is read again.
    pub shown: Option<Hook>,
    /// Whether it is offered again on this start, though offered before (agents connected under GitLance's name,
    /// once); none, never.
    pub again: Option<Asks>,
}

/// What a part says when asked (`Part::done`, `Part::again`).
pub type Asks = Rc<dyn Fn(&App) -> bool>;

/// A part's body (`Part::body`).
pub type Body = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Set up in one tab, the shell's (in the tab's services).
pub trait SetUp {
    /// The parts offered in this tab, handed once, as its parts are made.
    fn offer(&self, parts: Vec<Part>, cx: &mut App);
    /// Every part (Help → Set Up Artoo…), on the one keyed `at` when there is one.
    fn open(&self, at: Option<&str>, cx: &mut App);
    /// The first start: the parts not offered before and those offered again; nothing opens when none is.
    fn open_due(&self, cx: &mut App);
    fn is_open(&self, cx: &App) -> bool;
    /// ↵: the next part, or Done on the last.
    fn next(&self, cx: &mut App);
    /// Later, Done, esc, Settings opened over it: what was under it comes back. Whether it was open.
    fn close(&self, cx: &mut App) -> bool;
    /// The page while it is open.
    fn view(&self, cx: &App) -> Option<AnyView>;
}
