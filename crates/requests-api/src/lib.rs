//! Merge requests' contract (#165 stage 4): the routes of a request's pages — the request, a commit of it, a run of
//! its commits, two of its versions compared —, which anyone opens through the `Frame`; and what other features ask
//! of the requests read (`Requests`, a tab's service).

use artoo_core::git::{BranchRef, Version};
use artoo_core::route::Place;
use gpui::App;
use serde::{Deserialize, Serialize};

/// What other features ask of a tab's merge requests, from its services (`Rc<dyn Requests>`).
pub trait Requests {
    /// The request on branch `branch` (a local or remote one by its name, a request's own ref by its number): its
    /// number and title.
    fn of_branch(&self, branch: &BranchRef, cx: &App) -> Option<(u64, String)>;

    /// The request open, by its number: the one whose branch is read.
    fn current(&self, cx: &App) -> Option<u64>;

    /// The list is being read.
    fn listing(&self, cx: &App) -> bool;

    /// A project without a token: connecting is offered instead of the list.
    fn connectable(&self, cx: &App) -> bool;

    /// Of `numbers`, those the list read lacks (merged, closed, gone); none while the list failed.
    fn unlisted(&self, numbers: &[u64], cx: &App) -> Vec<u64>;

    /// Esc from request `number`'s page back to the list: its row is picked and brought into view.
    fn back_to_list(&self, number: u64, cx: &mut App);

    /// The versions the host keeps of the request open (its number with them), once read.
    fn versions(&self, cx: &App) -> Option<(u64, Vec<Version>)>;
}

/// A merge request, by its number.
#[derive(Serialize, Deserialize)]
pub struct RequestRoute {
    pub number: u64,
}

impl Place for RequestRoute {
    const KIND: &'static str = "requests.request";
}

/// A commit of a merge request.
#[derive(Serialize, Deserialize)]
pub struct RequestCommitRoute {
    pub number: u64,
    pub commit: String,
}

impl Place for RequestCommitRoute {
    const KIND: &'static str = "requests.commit";
}

/// A run of a merge request's commits, newest and oldest.
#[derive(Serialize, Deserialize)]
pub struct RequestCommitsRoute {
    pub number: u64,
    pub newest: String,
    pub oldest: String,
}

impl Place for RequestCommitsRoute {
    const KIND: &'static str = "requests.commits";
}

/// Two versions of a merge request compared.
#[derive(Serialize, Deserialize)]
pub struct RequestVersionsRoute {
    pub number: u64,
    pub from: usize,
    pub to: usize,
}

impl Place for RequestVersionsRoute {
    const KIND: &'static str = "requests.versions";
}
