//! Merge requests' contract (#165 stage 4): the routes of a request's pages — the request, a commit of it, a run of
//! its commits, two of its versions compared —, which anyone opens through the `Frame`; and what other features ask
//! of the requests read (`Requests`, a tab's service).

use artoo_core::git::BranchRef;
use artoo_core::route::Place;
use gpui::App;
use serde::{Deserialize, Serialize};

/// What other features ask of a tab's merge requests, from its services (`Rc<dyn Requests>`).
pub trait Requests {
    /// The request on branch `branch` (a local or remote one by its name, a request's own ref by its number): its
    /// number and title.
    fn of_branch(&self, branch: &BranchRef, cx: &App) -> Option<(u64, String)>;
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
