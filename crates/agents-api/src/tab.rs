//! A tab's agents (#165 stage 4): the chats of its repository, the reviewer's comments to them, the agents' notes on
//! its changes, the card asking which chat, Claude Code being connected. The tab makes them (`tab`) and keeps them;
//! the agents' parts in it — the Agents window, the card, the notes' summary, Settings → Agents — draw the same.

use crate::bus::{Chat, Mark};
use crate::comment::{Comment, Place, State};
use crate::notes::{Notes, Shown};
use artoo_core::rows::FileData;
use gpui::{App, Global, Rgba};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What the tab is told: the notes or the comments changed, their lines were found.
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

    /// The marks agents left on the project's requests — a pending comment of theirs, a request gathered (#148,
    /// #150) —, as last read (on the beat, when they changed).
    fn marks<'a>(&self, cx: &'a App) -> &'a [Mark];

    /// Agent `agent`'s mark: its letter on the hue of its newest chat here, else the accent.
    fn agent_mark(&self, agent: &str, cx: &App) -> (char, Rgba);

    /// The poll's beat: `root`'s chats read again, and what a chat in a worktree of its own changed when it is time
    /// (`here` the tab's working tree); the chats that listen through the app handed what was sent to them (each
    /// provider's beat, #18); its marks read again when they changed. Whether any of it changed, and what to say of
    /// each chat that began listening or went.
    fn beat(&self, root: Option<&Path>, here: Option<PathBuf>, cx: &mut App)
    -> (bool, Vec<String>);

    /// The reviewer's comments to agents, oldest first.
    fn comments<'a>(&self, cx: &'a App) -> &'a [Comment];

    /// Project `root`'s comments, read anew; its marks on the next beat.
    fn load_comments(&self, root: PathBuf, cx: &mut App);

    /// `comment` added to the drafts as the next one (its id, given back); comment `id` deleted; `comment` put in by
    /// its id, where it was (a draft deleted brought back); comment `id`'s text edited.
    fn add_comment(&self, comment: Comment, cx: &mut App) -> u64;
    fn delete_comment(&self, id: u64, cx: &mut App);
    fn restore_comment(&self, comment: Comment, cx: &mut App);
    fn edit_comment(&self, id: u64, body: String, cx: &mut App);

    /// The value typed on the agent's form on comment `id`, in a field of its own (it replaces any pick), and the
    /// note under the form: each a draft again, to go with the next send (#10 C).
    fn answer_value(&self, id: u64, value: &str, cx: &mut App);
    fn answer_note(&self, id: u64, note: &str, cx: &mut App);

    /// Whether folder `path` has a note for the agent not yet resolved or put away (#97).
    fn folder_waits(&self, path: &str, cx: &App) -> bool;

    /// Where `comment` sits in file `path` built as `data`: by its line's text, else, its line changed since, where
    /// that line went (`follow_comments`); nowhere once archived (#76) or in another file.
    fn place(&self, comment: &Comment, path: &str, data: &FileData, cx: &App) -> Option<Place>;

    /// Where comment `id`, its line changed since it was written, is on the head shown (`follow_comments`).
    fn comment_line(&self, id: u64, cx: &App) -> Option<u32>;

    /// Follows the line of every comment on the new side of files `paths` written on another commit than `head` (the
    /// one shown), from that commit to it, in repository `root`, in the background (#46).
    fn follow_comments(
        &self,
        root: Option<PathBuf>,
        head: Option<String>,
        paths: Vec<String>,
        cx: &mut App,
    );

    /// The open comments as markdown for an agent, each at its place `place`; `repo` the repository's name.
    fn export_comments(&self, repo: &str, place: &dyn Fn(&Comment) -> Place, cx: &App) -> String;

    /// The reviewer's pick on the agent's form on comment `id` — `None` their own call, else option `pick` —, an
    /// Order's item `at` moved one place up (or down), a digit on the form (0 their own call, 1…9 the option drawn
    /// with that number; whether it took it): a draft again, to go with the next send (#10 C).
    fn pick_answer(&self, id: u64, pick: Option<usize>, cx: &mut App);
    fn move_answer(&self, id: u64, at: usize, up: bool, cx: &mut App);
    fn answer_by_key(&self, id: u64, digit: usize, cx: &mut App) -> bool;

    /// Comment `id` resolved, or a draft again to be sent with what was said so far; sent and not taken yet, a draft
    /// with no chat (`withdraw`); archived or back; every resolved one archived.
    fn set_state(&self, id: u64, state: State, cx: &mut App);
    fn withdraw(&self, id: u64, cx: &mut App);
    fn archive(&self, id: u64, on: bool, cx: &mut App);
    fn archive_resolved(&self, cx: &mut App);

    /// `ids` each sent to its route (#77) — the chat working on the branch it was written on, else project `main`'s
    /// default —, the chats read again first; those no chat listening takes, for the card to ask.
    fn send(&self, ids: Vec<u64>, main: &Path, cx: &mut App) -> Vec<u64>;

    /// `ids` sent to chat `session` picked on the card, "" a new chat opened in a terminal in `root` (`main` the
    /// project); why not when the terminal could not open.
    fn send_to(
        &self,
        ids: Vec<u64>,
        session: &str,
        title: &str,
        root: &Path,
        main: &Path,
        cx: &mut App,
    ) -> Option<String>;

    /// Another chat than the page's (`page`, a session) changing file `path` (#77): one in a worktree of its own
    /// changed it there (#117: what it really edits); one in this working tree, where whose edit is whose cannot be
    /// told, took a note of its on the file and has not answered yet. Its name, letter and hue.
    fn changing(&self, path: &str, page: Option<&str>, cx: &App) -> Option<(String, char, Rgba)>;

    /// What the tab is told when the agents changed the comments — a comment added, a pick, a send, an archive — or
    /// found their lines.
    fn on_comments(&self, changed: OnChange, cx: &mut App);

    /// The reviewer's comments an agent answered that wait for them, for the Inbox: one it asks on, one it says it
    /// did, a draft whose answer was picked again.
    fn inbox_answers(&self, cx: &App) -> Vec<artoo_core::notice::Answer>;

    /// What the agents answered since the last look, taken in and kept: each one's state and replies as they were,
    /// when anything changed.
    fn take_answers(&self, cx: &mut App) -> Option<Vec<(u64, State, usize)>>;

    /// The agents' notes on the project's changes.
    fn notes<'a>(&self, cx: &'a App) -> &'a [Notes];

    /// Where note `id`, on a head the diff has moved on from, is on the head shown (`follow`).
    fn followed(&self, id: u64, cx: &App) -> Option<u32>;

    /// An agent's notes on its changes kept, its earlier ones on the same changes replaced; note `id` dismissed (its
    /// notes go with their last); a summary's notes dismissed with it, `uid` theirs.
    fn take_notes(&self, notes: Notes, cx: &mut App);
    fn dismiss_note(&self, id: u64, cx: &mut App);
    fn dismiss_notes(&self, uid: &str, cx: &mut App);

    /// Project `project`'s notes, read anew.
    fn load_notes(&self, project: PathBuf, cx: &mut App);

    /// Follows the line of every note on the changes `shown` (with the page's `branch`) whose head they moved on
    /// from, from the commit it was written on to the one shown, in repository `root`, in the background.
    fn follow(
        &self,
        root: Option<PathBuf>,
        shown: Option<Shown>,
        branch: Option<String>,
        cx: &mut App,
    );

    /// What the tab is told when the notes changed — taken, dismissed — or their lines were found.
    fn on_notes(&self, changed: OnChange, cx: &mut App);

    /// The card asking which chat `ids` go to: the chats of `project`, Claude Code's recent ones in `root`, a new
    /// chat.
    fn pick_chat(&self, ids: Vec<u64>, root: &Path, project: &Path, cx: &mut App);

    /// The card asking which chat `branch`'s notes go to; `now` the one they go to.
    fn pick_route(&self, branch: String, now: Option<String>, cx: &mut App);

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
