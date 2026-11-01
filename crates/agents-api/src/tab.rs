//! A tab's agents (#165 stage 4): the chats of its repository, the reviewer's comments to them, the agents' notes on
//! its changes, the card asking which chat, Claude Code being connected. The tab makes them (`tab`) and keeps them;
//! the agents' parts in it — the Agents window, the card, the notes' summary, Settings → Agents — draw the same.

use crate::bus::Chat;
use crate::comment::{Comment, State};
use crate::notes::{Notes, Shown};
use gpui::{App, Global, Rgba};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What the tab is told: the notes changed or their lines were found, a connect began or ended.
pub type OnChange = Rc<dyn Fn(&mut App)>;

/// One tab's agents.
pub trait Agents {
    /// The chats of the tab's repository, as last read.
    fn chats<'a>(&self, cx: &'a App) -> &'a [Chat];

    /// The sessions listening now.
    fn listening(&self, cx: &App) -> HashSet<String>;

    /// A chat's mark (#77): its agent's first letter on a hue of its own.
    fn mark(&self, session: &str, cx: &App) -> (char, Rgba);

    /// What each chat working in a worktree of its own has changed there, by session (#117).
    fn edits<'a>(&self, cx: &'a App) -> &'a HashMap<String, HashSet<String>>;

    /// The chat a note on `branch` goes to, with the one picked for it; `main` the project.
    fn route_of(&self, branch: Option<&str>, main: &Path, cx: &App) -> Option<Chat>;

    /// `root`'s chats read again; whether they changed.
    fn refresh(&self, root: Option<&Path>, cx: &mut App) -> bool;

    /// The poll's beat: `root`'s chats read again, and what a chat in a worktree of its own changed when it is time
    /// (`here` the tab's working tree). Whether they changed, and what to say of each that began listening or went.
    fn beat(&self, root: Option<&Path>, here: Option<PathBuf>, cx: &mut App)
    -> (bool, Vec<String>);

    /// The reviewer's comments to agents, oldest first.
    fn comments<'a>(&self, cx: &'a App) -> &'a [Comment];

    /// The comments changed; kept with `save_comments`.
    fn change_comments(&self, change: &mut dyn FnMut(&mut Vec<Comment>), cx: &mut App);

    /// Project `root`'s comments, read anew.
    fn load_comments(&self, root: PathBuf, cx: &mut App);

    /// The comments kept for their project, as they are now.
    fn save_comments(&self, cx: &App);

    /// What the agents answered since the last look, taken in and kept: each one's state and replies as they were,
    /// when anything changed.
    fn take_answers(&self, cx: &mut App) -> Option<Vec<(u64, State, usize)>>;

    /// The agents' notes on the project's changes.
    fn notes<'a>(&self, cx: &'a App) -> &'a [Notes];

    /// Where note `id`, on a head the diff has moved on from, is on the head shown (`follow`).
    fn followed(&self, id: u64, cx: &App) -> Option<u32>;

    /// The notes changed; kept with `save_notes`.
    fn change_notes(&self, change: &mut dyn FnMut(&mut Vec<Notes>), cx: &mut App);

    /// Project `project`'s notes, read anew.
    fn load_notes(&self, project: PathBuf, cx: &mut App);

    /// The notes kept for their project, as they are now.
    fn save_notes(&self, cx: &App);

    /// Follows the line of every note on the changes `shown` (with the page's `branch`) whose head they moved on
    /// from, from the commit it was written on to the one shown, in repository `root`, in the background.
    fn follow(
        &self,
        root: Option<PathBuf>,
        shown: Option<Shown>,
        branch: Option<String>,
        cx: &mut App,
    );

    /// What the tab is told when the notes' lines were found.
    fn on_followed(&self, done: OnChange, cx: &mut App);

    /// The card asking which chat `ids` go to: the chats of `project`, Claude Code's recent ones in `root`, a new
    /// chat.
    fn pick_chat(&self, ids: Vec<u64>, root: &Path, project: &Path, cx: &mut App);

    /// The card asking which chat `branch`'s notes go to; `now` the one they go to.
    fn pick_route(&self, branch: String, now: Option<String>, cx: &mut App);

    /// Claude Code being connected (Set up's Agents): whether it is under way, and what the last one said.
    fn connecting(&self, cx: &App) -> (bool, Option<String>);

    /// Artoo's MCP server added to every Claude Code profile, in the background.
    fn connect(&self, cx: &mut App);

    /// What the tab is told when a connect began or ended.
    fn on_connecting(&self, changed: OnChange, cx: &mut App);

    /// The handle itself, for the agents' parts in the tab.
    fn as_any(&self) -> &dyn Any;
}

/// How a tab's agents are made.
pub type Make = Rc<dyn Fn(&mut App) -> Rc<dyn Agents>>;

struct Installed(Make);

impl Global for Installed {}

/// Sets how every tab's agents are made (the agents' `install`).
pub fn provide(cx: &mut App, make: Make) {
    cx.set_global(Installed(make));
}

/// A new tab's agents, as provided.
pub fn tab(cx: &mut App) -> Rc<dyn Agents> {
    let make = cx.global::<Installed>().0.clone();
    make(cx)
}
