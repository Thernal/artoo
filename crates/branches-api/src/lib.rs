//! Branches' contract (#165 stage 4): the routes of a branch's pages — the branch, a commit of it, a run of its
//! commits, two of its versions compared —, of two refs compared and of the working tree, which anyone opens through
//! the `Frame`; and the branches listed, as a part reads them (`Branches`).

use artoo_core::git::BranchRef;
use artoo_core::route::{Place, Route};
use gpui::{AnyElement, App};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
