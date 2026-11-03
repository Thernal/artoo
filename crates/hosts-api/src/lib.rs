//! The hosts' contract (#165 stage 3): what GitLab and GitHub answer in the same shapes — requests and their
//! discussions, reactions, pending comments, reviews and approvals, pipelines, pushes, what stands in a merge's
//! way and what a merge sends — and the traits that read them (`Host`), found for a remote through `Hosts`.
//!
//! - `hosts::get(cx).of(&remote)` is the host a repository's remote is on; a worker takes a clone of the
//!   `Arc` before it leaves the window's thread, a windowless mode is handed `Arc<dyn Hosts>`.
//! - A function that needs one side of a host takes it alone (`&dyn Discussions`): `Host` is the five, and a
//!   `&dyn Host` gives each.
//! - Every call blocks (curl): off the window's thread.

pub mod connect;

use anyhow::Result;
use artoo_core::git::WebRemote;
use gpui::{App, Global};
use std::collections::HashMap;
use std::sync::Arc;

pub use artoo_core::mr::*;

/// The hosts Artoo knows: the one a remote is on.
pub trait Hosts: Send + Sync {
    /// The host `remote` is on: GitHub's for github.com, GitLab's for any other.
    fn of(&self, remote: &WebRemote) -> Arc<dyn Host>;
}

/// One remote's host, every side of it.
pub trait Host: Account + Requests + Discussions + Reviews + Merging {
    /// The remote it serves.
    fn remote(&self) -> &WebRemote;
}

/// Who the host is and the token Artoo reads it with.
pub trait Account: Send + Sync {
    /// The host the requests go to, without the scheme.
    fn host(&self) -> String;

    /// `https://host` of the remote's host: what its tokens are kept by.
    fn api_host(&self) -> String;

    /// Where the token for a GitLab host comes from, best first: `GITLAB_TOKEN`, the token saved by this
    /// app for that host (pasted, made with the SSH key or copied from an IDE), the way picked for it
    /// that borrows one — the GitLab CLI's or what git keeps (#142, #156) — then the shared
    /// `~/.config/gitlab-token`. What the window's thread asks: a CLI's token only as read this run
    /// (`token` reads it).
    fn token_source(&self) -> Option<(String, &'static str)>;

    /// The token for the host; a CLI's is read when not read yet this run (a process: off the
    /// window's thread — there, `token_now`).
    fn token(&self) -> Option<String>;

    /// The token for the host as known now, nothing run: what the window's thread asks.
    fn token_now(&self) -> Option<String>;

    /// Keeps `token` for the host of `remote`, readable by the owner only.
    fn save_token(&self, token: &str) -> Result<()>;

    /// Removes the token this app saved for the host of `remote` (a wrong one, after a failed test).
    fn forget_token(&self);

    /// Where a token for this host is made, with the name and the scope filled in.
    fn token_page(&self) -> String;

    /// The name the token belongs to: proof that the host, the token and the network work.
    fn me(&self) -> Result<String>;

    /// Who `token` is on the remote's host — its name and username — asked with that token rather
    /// than the one in use (#156).
    fn whois_with(&self, token: &str) -> Result<(String, String)>;

    /// What the token can do and until when (GitLab: "api, read_repository · expires 2027-09-01");
    /// `None` where the host does not say (GitHub).
    fn token_info(&self) -> Option<String>;

    /// `token`'s scopes and the day it ends, asked with that token (#156); `None` on GitHub or where
    /// the host does not say.
    fn token_check_with(&self, token: &str) -> Option<(Vec<String>, Option<String>)>;

    /// Revokes the token in use: the one a renewed token replaces (#142).
    fn revoke_token(&self) -> Result<()>;

    /// Whether merge requests can be read for this remote at all, as known now (the window's
    /// thread: a CLI's token only as read this run).
    fn available(&self) -> bool;

    /// The token's username (`ARTOO_MR_FIXTURE`: `user.json`, else "you").
    fn username(&self) -> Result<String>;
}

/// A project's requests: the open and the past, one, what a new one needs, its pipeline and pushes, the to-do list.
pub trait Requests: Send + Sync {
    /// Every open request: page after page, until one comes short (#114).
    fn list(&self) -> Result<Vec<Mr>>;

    /// The page at `at` (none: the first) of `whose` merged or closed requests, newest first
    /// (#114, #169).
    fn list_past(&self, state: MrState, whose: &Whose, at: Option<&str>) -> Result<PastPage>;

    /// One request, whatever its state: one that left the open list, to say what became of it.
    fn one(&self, iid: u64) -> Result<Mr>;

    /// Request `iid`'s description, labels in their colours, milestone and assignees.
    fn details(&self, iid: u64) -> Result<Details>;

    /// The project's labels in their colours, for a new request.
    fn labels(&self) -> Result<Vec<Label>>;

    /// The project's open milestones, for a new request.
    fn milestones(&self) -> Result<Vec<Milestone>>;

    /// The project's members (#51): username and name, for `@` completion.
    fn members(&self) -> Result<Vec<(String, String)>>;

    /// The description template set in the project's settings (GitLab's `merge_requests_template`);
    /// GitHub has none apart from the files.
    fn project_template(&self) -> Result<Option<String>>;

    /// The members a mention does not reach: in a private project, those below Reporter cannot read
    /// its merge requests, so GitLab sends them nothing. GitHub: none (only collaborators are listed).
    fn unreached(&self) -> Result<Vec<String>>;

    /// Opens a merge request from `source` into `target`. The branch must already be on the remote.
    fn create(
        &self,
        source: &str,
        target: &str,
        title: &str,
        description: &str,
        options: &NewOptions,
    ) -> Result<Mr>;

    /// The merge request's newest pipeline; `None` when it has none.
    fn pipeline(&self, iid: u64) -> Result<Option<Pipeline>>;

    /// The versions (pushes) of a merge request, oldest first.
    fn versions(&self, iid: u64) -> Result<Vec<Push>>;

    /// The user's pending to-dos on merge requests of this project, newest first.
    fn todos(&self) -> Result<Vec<Todo>>;

    /// Marks to-do `id` done on GitLab.
    fn mark_todo_done(&self, id: u64) -> Result<()>;
}

/// A request's discussions: their notes and reactions, the pending comments, the files they link.
pub trait Discussions: Send + Sync {
    fn threads(&self, iid: u64) -> Result<Vec<Thread>>;

    /// The reactions on `notes` of request `iid`, by note id; a note is `(id, on the conversation)`
    /// (GitHub keeps the request's own comments apart from review comments).
    fn awards(&self, iid: u64, notes: &[(u64, bool)]) -> Result<HashMap<u64, Vec<Award>>>;

    /// Gives note `note` the reaction `name`; its id comes back.
    fn award(&self, iid: u64, note: u64, issue: bool, name: &str) -> Result<u64>;

    /// Takes one's reaction `award` (named `name`) back from note `note`.
    fn unaward(&self, iid: u64, note: u64, issue: bool, name: &str, award: u64) -> Result<()>;

    /// Posts `body` as a new discussion on a line of merge request `iid`, whose head the reviewer sees
    /// as `head`, or keeps it pending (`draft`): then the pending comment's id comes back (#150).
    /// Refuses when GitLab's head is another commit: the line numbers would not mean the same thing
    /// there.
    fn post_discussion(
        &self,
        iid: u64,
        head: &str,
        place: &Place<'_>,
        body: &str,
        draft: bool,
    ) -> Result<Option<u64>>;

    /// Marks a discussion resolved or open again.
    fn resolve(&self, iid: u64, discussion: &str, resolved: bool) -> Result<()>;

    /// Adds a reply to a discussion.
    fn reply(&self, iid: u64, discussion: &str, body: &str) -> Result<()>;

    /// Changes the text of one's own note `note` in discussion `discussion` of request `iid`.
    fn edit_note(&self, iid: u64, discussion: &str, note: u64, body: &str) -> Result<()>;

    /// Deletes one's own note `note` of discussion `discussion` on request `iid` (#76).
    fn delete_note(&self, iid: u64, discussion: &str, note: u64) -> Result<()>;

    /// The author's own unpublished comments on a request.
    fn drafts(&self, iid: u64) -> Result<Vec<Draft>>;

    /// Publishes draft `id` of the request alone: it becomes a discussion, the others stay pending
    /// (#138). GitHub publishes a pending comment only with its review.
    fn publish_draft(&self, iid: u64, id: u64) -> Result<()>;

    fn delete_draft(&self, iid: u64, id: u64) -> Result<()>;
}

/// Approving a request and submitting a review.
pub trait Reviews: Send + Sync {
    /// Who approved request `iid`, and whether the token's user did or may.
    fn approvals(&self, iid: u64) -> Result<Approvals>;

    /// Approves request `iid` as of `head`: GitLab refuses when its head moved on since.
    fn approve(&self, iid: u64, head: &str) -> Result<()>;

    /// Takes one's approval of request `iid` back.
    fn unapprove(&self, iid: u64) -> Result<()>;

    /// A comment on request `iid` as a whole (a review's summary).
    fn comment(&self, iid: u64, body: &str) -> Result<()>;

    /// Submits a review of request `iid` (whose head the reviewer sees as `head`): one's pending
    /// comments when there are, the summary, the verdict. GitLab takes them as three calls; GitHub as
    /// one review. `approved` says the reviewer approved already, so approving again is skipped.
    fn submit(
        &self,
        iid: u64,
        head: &str,
        drafts: bool,
        summary: &str,
        verdict: Verdict,
        approved: bool,
    ) -> Result<()>;
}

/// What stands in a merge's way, the merge, and what helps it along.
pub trait Merging: Send + Sync {
    /// What GitLab says of merging request `iid` (#164): its status, the project's rules and its
    /// defaults. The fixture's are `merge-<iid>.json` and `project.json`.
    fn merge_state(&self, iid: u64) -> Result<MergeState>;

    /// Merges request `iid` as of the head that was reviewed (#164): GitLab refuses when the branch
    /// moved on since. Or sets it to merge when the pipeline succeeds.
    fn merge(&self, iid: u64, m: &Merge) -> Result<()>;

    /// Request `iid` no longer merges when its pipeline succeeds; `node` is GitHub's id of it.
    fn cancel_auto_merge(&self, iid: u64, node: &str) -> Result<()>;

    /// GitLab puts the request's commits on top of its target; the pipeline runs again.
    fn rebase(&self, iid: u64) -> Result<()>;
}

struct Installed(Arc<dyn Hosts>);

impl Global for Installed {}

/// Sets the hosts every window reads (the implementation's `install`; a test's own).
pub fn provide(cx: &mut App, hosts: Arc<dyn Hosts>) {
    cx.set_global(Installed(hosts));
}

/// The hosts, as provided.
pub fn get(cx: &App) -> Arc<dyn Hosts> {
    cx.global::<Installed>().0.clone()
}
