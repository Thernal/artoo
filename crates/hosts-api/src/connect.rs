//! Connecting a tab's host (#142, #156): the ways in this computer offers, easiest first, and Settings → Merge
//! requests' list of them, drawn by hosts. The tab hands where it is with each ask — it asks from inside its own
//! update, where the panel cannot read it — and is told when a way connected.

use artoo_core::git::WebRemote;
use gpui::{AnyElement, App};
use std::path::PathBuf;
use std::rc::Rc;

/// The panel's field, one of the tab's inputs (`Input::id`).
pub const INPUT: &str = "hosts.token";

/// Settings → Merge requests, the hosts' section: the host's ways in to pick from (#156). Its id is the one the
/// window root's section had, so a toast's button and a kept place still open it.
pub const SETTINGS: &str = "requests";

/// Where the tab is: its repository, its host's address, and whether the panel's field has its keyboard.
#[derive(Clone, Default)]
pub struct Here {
    pub root: Option<PathBuf>,
    pub web: Option<WebRemote>,
    pub typing: bool,
}

/// A way connected the host: the requests are read again; `Some`, what to say of it ("Connected as @ada · …"),
/// `None` when a borrowed token was read before them.
pub type OnConnected = Rc<dyn Fn(Option<String>, &mut App)>;

/// The tab's connect panel, a service its features offer (`Services`).
pub trait Connect {
    /// What the tab is told when a way connected.
    fn on_connected(&self, done: OnConnected, cx: &mut App);

    /// No token for the host, or Settings → Merge requests opened: the ways in are looked for, once a run.
    fn look(&self, here: Here, cx: &mut App);

    /// Whether the way picked for `web`'s host borrows a token (the CLI's, git's) not read yet this run.
    fn to_borrow(&self, web: &WebRemote) -> bool;

    /// That token read, then the tab told (`OnConnected` with `None`).
    fn borrow(&self, here: Here, cx: &mut App);

    /// The token Artoo made renewed a few days before its end, once a run, quietly.
    fn renew(&self, here: Here, cx: &mut App);

    /// The panel on Merge requests' page while the host has no token.
    fn panel(&self, here: Here, cx: &mut App) -> AnyElement;

    /// Settings → Merge requests: the host's ways to pick from.
    fn settings(&self, here: Here, cx: &mut App) -> Option<AnyElement>;

    /// The panel's field given the keyboard: its ↑ ↓ walk the ways, ↵ presses the one focused.
    fn take_keyboard(&self, cx: &mut App);
}
