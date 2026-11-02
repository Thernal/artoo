//! Branches' contract (#165 stage 4): the routes of a branch's pages — the branch, a commit of it, a run of its
//! commits, two of its versions compared —, of two refs compared and of the working tree, which anyone opens through
//! the `Frame`; and the branches listed, as a part reads them (`Branches`).

use artoo_core::git::{BranchRef, CommitInfo, Deco};
use artoo_core::route::{Place, Route};
use gpui::{AnyElement, App};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// What of the branch open the diff shows: a commit, a run of commits (indexes, newest first), two versions, the
/// working tree, or nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    None,
    Commit(usize),
    /// A run of the list's commits as one diff: the newest's index and the oldest's.
    Commits {
        newest: usize,
        oldest: usize,
    },
    Versions {
        from: usize,
        to: usize,
    },
    WorkingTree,
}

/// What showing a route came to (`Branches::take_shown`), for what the tab does after.
pub enum Shows {
    /// Branch `ix` (of `Branches::list`) opened, `read` anew when there is one to read; what follows its read is the
    /// branches' own.
    Branch { ix: usize, read: bool },
    /// Request `number` to open: what it is read from waits for the requests (the tab's for now).
    Request(u64),
    /// Shown: the commit it is, kept among the places opened lately; `follow` when the page on screen follows
    /// what is shown.
    Shown {
        commit: Option<CommitInfo>,
        follow: bool,
    },
    /// Nothing changed.
    Stay,
}

/// A page waiting, shown (`Branches::finish_waiting`): the commit it shows, when it shows one.
pub struct Finished {
    pub commit: Option<CommitInfo>,
}

/// What showing a branch read came to (`Branches::show_read`).
pub struct Settled {
    /// The commits shown, in order: the tab keeps them among the places opened lately.
    pub commits: Vec<CommitInfo>,
    /// A page of the branch or of the request on it is shown, which the page on screen follows; not the branch's
    /// own Changes.
    pub follow: bool,
}

/// What the tab hears of the branches' reads (`Branches::on_event`).
#[derive(Clone)]
pub enum Event {
    /// The branch open was read: the tab shows it (`Branches::show_read`).
    Read,
    /// More of its history is listed.
    Older,
    /// ↵ in the commits' search: commit `ix`, the first found, shown.
    Picked(usize),
    /// A read failed: what to say.
    Failed(String),
    /// The working tree changed: what shows it reads it again.
    Working,
    /// The working tree's poll came round, changed or not: what rides on its beat looks (the agents' answers,
    /// pictures come in).
    Beat,
    /// The repository re-read after it changed under the window (`Branches::reload`): what it found.
    Reloaded(Reloaded),
}

/// What a re-read of the repository found.
#[derive(Clone)]
pub enum Reloaded {
    /// No branch is open: the branches are listed anew, nothing else.
    Listed,
    /// The branch open is gone: the checked-out one, or the first, to fall back to.
    Gone {
        refname: String,
        fallback: Option<usize>,
    },
    /// Nothing new on the branch open.
    Same,
    /// Only the reflog moved: a new version of the same history; `seen` versions were listed before.
    Versions { seen: usize },
    /// New commits, held back until the user asks for them.
    Held,
    /// The repository could not be read (mid-rebase, for a moment): the next change tries again.
    Unreadable,
}

/// What showing the commits held back leaves to do with the diff shown (`Branches::apply_pending`).
pub enum Applied {
    /// The selection still holds, at its new index.
    Kept,
    /// The commit shown is gone: the newest is shown instead.
    Newest,
    /// What was shown no longer is: the diff goes.
    Cleared,
}

/// A row of Branches' list: the working tree, or a branch (#72), by its place in `Branches::list`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BranchRow {
    WorkingTree,
    Branch(usize),
}

/// What a part reads of the tab's branches (`Rc<dyn Branches>` in its services).
pub trait Branches {
    /// The branches listed, local and remote.
    fn list(&self, cx: &App) -> Arc<Vec<BranchRef>>;

    /// How many commits of the branch open are listed.
    fn commit_count(&self, cx: &App) -> usize;

    /// The versions of the branch open, oldest first: each one's number and tip.
    fn versions(&self, cx: &App) -> Vec<(usize, git2::Oid)>;

    /// Its commits as a page of their own, the version each came with on it when `versions`. Asked from a view's
    /// own drawing.
    fn commits_page(&self, versions: bool, cx: &mut App) -> AnyElement;

    /// The side panel of a branch's or a request's page, at `width` (`Pages::side`).
    fn panel(&self, width: f32, cx: &mut App) -> Option<AnyElement>;

    /// A branch's, a comparison's, the working tree's or a request's route on screen (`Pages::show`): what it shows
    /// read, unless it is what is loaded.
    fn show(&self, route: &Route, cx: &mut App);

    /// The route of what the branch open shows now: a request's page while the request open is on it, a
    /// comparison `on_screen` kept while a run of its commits narrows it; `None` while no branch is open.
    fn loaded(&self, on_screen: Option<&Route>, cx: &App) -> Option<Route>;

    /// Whether branch `refname`'s page has Commits and Changes as tabs: it is the one open, off the main line.
    fn tabbed(&self, refname: &str, cx: &App) -> bool;

    /// A commit, or a run of them (newest, oldest), came on screen as a page: its Commits page has it picked.
    fn pick_commits(&self, newest: git2::Oid, oldest: Option<git2::Oid>, cx: &mut App);

    /// A request's diff waits for its branch to be read: what is shown does not move the page yet.
    fn settling(&self, cx: &App) -> bool;

    /// Another page came on screen: nothing waits to be shown.
    fn stop_waiting(&self, cx: &mut App);

    /// Esc from `route`'s page back to the list: its row is picked (the working tree's, a branch's).
    fn back_to_list(&self, route: &Route, cx: &mut App);

    /// The page `selection` of the branch open is once shown: a commit, a run or two versions, of the request open
    /// while it is on the branch. None for a selection that is not one of those.
    fn route_of(&self, selection: Selection, cx: &App) -> Option<Route>;

    /// What showing the last route came to, taken once (`Pages::show` said it).
    fn take_shown(&self, cx: &mut App) -> Option<Shows>;

    /// The page waiting shown, once what it asks for is in (a request's commits, a branch's versions); `None`
    /// while it waits, or when none does.
    fn finish_waiting(&self, cx: &mut App) -> Option<Finished>;

    /// A project opened at `root` with its `branches`, the refs' decorations when it is a repository; `fresh`
    /// when it is another than before (the commits' search starts over).
    fn opened(
        &self,
        root: PathBuf,
        branches: Vec<BranchRef>,
        decor: Option<HashMap<git2::Oid, Vec<Deco>>>,
        fresh: bool,
        cx: &mut App,
    );

    /// The branch to open first: the one named `refname`, else the checked-out one, else the first listed.
    fn first_branch(&self, refname: Option<&str>, cx: &App) -> Option<usize>;

    /// Branch `ix` (of `list`) opened at commit `keep`, what the one before showed forgotten; whether it is read
    /// anew.
    fn open_branch(&self, ix: usize, keep: Option<git2::Oid>, cx: &mut App) -> bool;

    /// What the branch open showed forgotten: no branch open.
    fn forget(&self, cx: &mut App);

    /// The diff of the branch open let go; the branch stays open.
    fn clear_diff(&self, cx: &mut App);

    /// Commit `ix` of the branch open shown, `keep` the file to open in it: the commit, when there is one.
    fn show_commit(&self, ix: usize, keep: Option<String>, cx: &mut App) -> Option<CommitInfo>;

    /// A run of its commits as one diff (indexes, newest first); whether it was shown.
    fn show_commits(
        &self,
        newest: usize,
        oldest: usize,
        keep: Option<String>,
        cx: &mut App,
    ) -> bool;

    /// Two of its versions compared; whether they were.
    fn show_versions(&self, from: usize, to: usize, keep: Option<String>, cx: &mut App) -> bool;

    /// The interdiff of one modified commit of the comparison shown; whether it was shown.
    fn show_interdiff(&self, pair: usize, cx: &mut App) -> bool;

    /// The branch read shown (`Event::Read`): its page, the commit asked for, or the request waiting for it.
    fn show_read(&self, cx: &mut App) -> Option<Settled>;

    /// The branch open, by its place in `list`.
    fn branch(&self, cx: &App) -> Option<usize>;

    /// What of the branch open the diff shows.
    fn selection(&self, cx: &App) -> Selection;

    /// Commit `ix` of those listed of the branch open.
    fn commit(&self, ix: usize, cx: &App) -> Option<CommitInfo>;

    /// The listed commit whose id starts with `prefix`.
    fn commit_ix(&self, prefix: &str, cx: &App) -> Option<usize>;

    /// The commits' search given `text`: the list filtered.
    fn search(&self, text: String, cx: &mut App);

    /// `hear` hears what the branches' reads come to, for as long as the tab lives.
    fn on_event(&self, hear: std::rc::Rc<dyn Fn(&Event, &mut App)>, cx: &mut App);

    /// The repository changed under the window: the branches and the branch open read again in the background;
    /// what it found is said (`Event::Reloaded`).
    fn reload(&self, cx: &mut App);

    /// What was found on the branch open no longer applies (another one is open, or it is read anew), nor does a
    /// re-read under way.
    fn forget_news(&self, cx: &mut App);

    /// The commits held back shown, the selection kept where it still holds; with how many versions were listed
    /// before. `None` when nothing was held back.
    fn apply_pending(&self, cx: &mut App) -> Option<(Applied, usize)>;

    /// The branches a search lists, with their place in `list` (a request's fetched head is none of them).
    fn listed(&self, cx: &App) -> Vec<(usize, BranchRef)>;

    /// Branches' rows in the order the list shows them, `filter` typed.
    fn rows(&self, filter: &str, cx: &App) -> Vec<BranchRow>;

    /// The row picked in the list, else the one it picks when nothing is.
    fn picked_row(&self, cx: &App) -> Option<BranchRow>;

    /// `row` picked in the list (none: nothing); the working tree's files read when it is its row.
    fn pick_row(&self, row: Option<BranchRow>, cx: &mut App);

    /// "4 local · 6 on origin": the count after Branches' name in the bar.
    fn count_line(&self, cx: &App) -> String;

    /// A remote's branches folded or open in the list.
    fn toggle_remote(&self, remote: String, cx: &mut App);

    /// When each branch's tip was made and how far it is from the main line, read for the list's rows.
    fn read_facts(&self, cx: &mut App);

    /// The working tree's files read again when they changed since the last read.
    fn read_tree(&self, cx: &mut App);

    /// The commits' search opened (it is typed in the branches' own input).
    fn open_find(&self, cx: &mut App);

    /// The commits' search closed.
    fn close_find(&self, cx: &mut App);

    /// Something is typed in the commits' search.
    fn searching(&self, cx: &App) -> bool;

    /// The commits filtered again by what the search says.
    fn refilter(&self, cx: &mut App);

    /// The paths of every commit listed read, for the search's `path:`.
    fn index_paths(&self, cx: &mut App);

    /// The working tree looked at while the project is open (`plain` a folder without git, where only the beat
    /// runs; `shown` whether the tab is on screen).
    fn start_working_poll(
        &self,
        plain: bool,
        shown: std::rc::Rc<std::cell::Cell<bool>>,
        cx: &mut App,
    );

    /// The working tree shown against `HEAD`, `keep` the file to stay on (a reload finds a file partly staged at
    /// its place, #22).
    fn show_working_tree(&self, keep: Option<String>, cx: &mut App);

    /// The working tree's halves: what is not staged only, or all of it.
    fn set_only_unstaged(&self, on: bool, cx: &mut App);
}

/// A branch, by its refname.
#[derive(Serialize, Deserialize)]
pub struct BranchRoute {
    pub refname: String,
}

impl Place for BranchRoute {
    const KIND: &'static str = "branches.branch";
}

/// A commit of a branch.
#[derive(Serialize, Deserialize)]
pub struct BranchCommitRoute {
    pub refname: String,
    pub commit: String,
}

impl Place for BranchCommitRoute {
    const KIND: &'static str = "branches.commit";
}

/// A run of a branch's commits, newest and oldest.
#[derive(Serialize, Deserialize)]
pub struct BranchCommitsRoute {
    pub refname: String,
    pub newest: String,
    pub oldest: String,
}

impl Place for BranchCommitsRoute {
    const KIND: &'static str = "branches.commits";
}

/// Two versions of a branch compared.
#[derive(Serialize, Deserialize)]
pub struct VersionsRoute {
    pub refname: String,
    pub from: usize,
    pub to: usize,
}

impl Place for VersionsRoute {
    const KIND: &'static str = "branches.versions";
}

/// What is not committed.
#[derive(Serialize, Deserialize)]
pub struct WorkingTreeRoute {}

impl Place for WorkingTreeRoute {
    const KIND: &'static str = "branches.working-tree";
}

/// Two refs compared, each with the commit it was at; since their merge base or directly.
#[derive(Serialize, Deserialize)]
pub struct CompareRoute {
    pub base: String,
    pub base_commit: String,
    pub head: String,
    pub head_commit: String,
    pub since: bool,
}

impl Place for CompareRoute {
    const KIND: &'static str = "branches.compare";
}
