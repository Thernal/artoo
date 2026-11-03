//! Merge requests' contract (#165 stage 4): the routes of a request's pages — the request, a commit of it, a run of
//! its commits, two of its versions compared —, which anyone opens through the `Frame`; and what other features ask
//! of the requests read (`Requests`, a tab's service).

use artoo_core::git::{BranchRef, Version};
use artoo_core::mr::{Approvals, Draft, Mr, Thread};
use artoo_core::route::Place;
use artoo_ui_kit::complete::Choice;
use gpui::{AnyElement, App};
use serde::{Deserialize, Serialize};

/// Which requests the list shows: All · To review · Mine in Merge requests' bar (#70, #72).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Scope {
    #[default]
    All,
    /// The token's user is asked to review.
    ReviewMe,
    /// The token's user wrote it.
    Mine,
}

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

    /// Request `number` wherever it was read: the open list, the merged or closed, those that left the list.
    fn find(&self, number: u64, cx: &App) -> Option<Mr>;

    /// The open list.
    fn list(&self, cx: &App) -> Vec<Mr>;

    /// The host has a token: the requests are read.
    fn available(&self, cx: &App) -> bool;

    /// The token's username: one's own notes are the ones it wrote.
    fn login(&self, cx: &App) -> Option<String>;

    /// The project's members, (username, name).
    fn members(&self, cx: &App) -> Vec<(String, String)>;

    /// Who approved the request open, and where you stand.
    fn approvals(&self, cx: &App) -> Option<Approvals>;

    /// The request open's discussions, discussion `ix`, and your pending comments on it.
    fn threads(&self, cx: &App) -> Vec<Thread>;
    fn thread(&self, ix: usize, cx: &App) -> Option<Thread>;
    fn drafts(&self, cx: &App) -> Vec<Draft>;

    /// Where pending comment `ix` is: its own place, or for a reply its discussion's — (path, new line, old line).
    fn draft_place(&self, ix: usize, cx: &App) -> Option<(String, Option<u32>, Option<u32>)>;

    /// The discussions on lines of file `path`: (index, old side, line).
    fn threads_in(&self, path: &str, cx: &App) -> Vec<(usize, bool, u32)>;

    /// What a long hover on request `number`'s title says.
    fn request_tip(&self, number: u64, cx: &App) -> Option<String>;

    /// The host's id of request `number`'s version at `head` (a link to two of them compared names the newer by it).
    fn version_id(&self, number: u64, head: git2::Oid, cx: &App) -> Option<u64>;

    /// Request `number` was read alone already (gone from the list, a link's): one try each.
    fn tried(&self, number: u64, cx: &App) -> bool;

    /// How many of the open list `scope` lists.
    fn count_in(&self, scope: Scope, cx: &App) -> usize;

    /// ☺+ is open; Merge's card is open.
    fn grid_open(&self, cx: &App) -> bool;
    fn merge_card_open(&self, cx: &App) -> bool;

    /// The list completing the token before `caret` in `text`, unless it was closed with esc at `closed`.
    fn completion(
        &self,
        text: &str,
        caret: usize,
        closed: Option<usize>,
        cx: &App,
    ) -> Option<(usize, Vec<Choice>)>;

    /// The files of request `number`'s whole diff when it was last loaded (its Changes tab's count); a diff of a
    /// request arrived.
    fn files(&self, number: u64, cx: &App) -> Option<usize>;
    fn set_files(&self, files: Option<(u64, usize)>, cx: &mut App);

    /// The row picked in the list.
    fn picked_row(&self, cx: &App) -> Option<u64>;

    /// The request open's approval chip and Merge's control, on its bar.
    fn approval_chip(&self, cx: &mut App) -> Option<AnyElement>;
    fn merge_control(&self, compact: bool, github: bool, cx: &mut App) -> Option<AnyElement>;
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
