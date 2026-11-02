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
