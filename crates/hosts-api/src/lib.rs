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
use artoo_core::git::{DiffLine, WebRemote};
use gpui::{App, Global};
use std::collections::HashMap;
use std::sync::Arc;

/// Where a merge request is in its life (#114): open, merged, or closed without merging.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum MrState {
    #[default]
    Opened,
    Merged,
    Closed,
}

impl MrState {
    pub fn word(self) -> &'static str {
        match self {
            MrState::Opened => "Open",
            MrState::Merged => "Merged",
            MrState::Closed => "Closed",
        }
    }
}

/// Merged and closed requests come a page of this many at a time, as the list is scrolled.
pub const PAST_PAGE: usize = 30;

/// Whose merged or closed requests a list reads. The host picks them, so a short list ends at its
/// own last page instead of reading the whole project's (#169).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Whose {
    All,
    /// Written by this username.
    Author(String),
    /// With this username among the reviewers.
    Reviewer(String),
}

/// A page of merged or closed requests: what is on it, where the next one starts (none: it was
/// the last), and how many there are in all where the host says.
#[derive(Debug, Default)]
pub struct PastPage {
    pub list: Vec<Mr>,
    pub next: Option<String>,
    pub total: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mr {
    pub iid: u64,
    pub title: String,
    pub author: String,
    pub source: String,
    pub target: String,
    pub url: String,
    /// Seconds since the epoch.
    pub updated: i64,
    pub created: i64,
    /// The source branch's head as GitLab knows it.
    pub head: String,
    pub comments: usize,
    pub draft: bool,
    /// The author's and the reviewers' usernames: whose request it is, who was asked to look.
    pub author_login: String,
    pub reviewers: Vec<String>,
    /// What the author wrote about it, as the list gives it: the root page's preview (#72).
    pub description: String,
    pub state: MrState,
    /// What it was merged as (the merge or squash commit): its first parent is the target as it
    /// was, so the diff is the request's, not what the target became since.
    pub merged_with: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// GitLab's id: a link's `#note_<id>` points at it.
    pub id: u64,
    pub author: String,
    /// The author's username, to tell one's own notes.
    pub login: String,
    pub body: String,
    pub created: i64,
}

/// A discussion on a merge request: on a line of a file, or on the request as a whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    /// GitLab's id of the discussion, for replying to it and resolving it.
    pub id: String,
    pub notes: Vec<Note>,
    pub path: Option<String>,
    /// 1-based line on the new side; `None` for a comment on a removed line or on no line.
    pub new_line: Option<u32>,
    pub old_line: Option<u32>,
    pub resolved: bool,
    /// A discussion on code can be resolved; a plain comment cannot.
    pub resolvable: bool,
    /// Who resolved it (name, username) and when, while it is resolved.
    pub resolved_by: Option<(String, String)>,
    pub resolved_at: i64,
    /// The commit (a version's head) the discussion was written on, when the host says (#46).
    pub commit: Option<String>,
    /// The host already knows its line is gone from the latest version (GitHub's `isOutdated`).
    pub outdated: bool,
}

/// The latest pipeline of a merge request, and when it failed, the job that did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline {
    /// GitLab's word: success, failed, running, pending, canceled, skipped, manual …
    pub status: String,
    pub url: String,
    /// Seconds since the epoch, when it last changed.
    pub updated: i64,
    /// The jobs that failed, each with its page; the first is the one the pill names.
    pub failed: Vec<(String, String)>,
}

/// A comment of one's own that is not published yet: only its author sees it until the review is
/// submitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Draft {
    pub id: u64,
    pub body: String,
    pub path: Option<String>,
    pub new_line: Option<u32>,
    pub old_line: Option<u32>,
    /// The discussion it replies to (its `Thread::id`): a reply in another's thread has no place of
    /// its own on GitLab, it is where its thread is (#166).
    pub reply_to: Option<String>,
}

/// A GitLab to-do on a merge request of this project: someone mentioned the user, asked for a
/// review, addressed them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Todo {
    pub id: u64,
    /// GitLab's word: mentioned, review_requested, directly_addressed, assigned, …
    pub action: String,
    pub iid: u64,
    pub title: String,
    pub author: String,
    /// The comment's text, when it came from one.
    pub body: String,
    /// The page it points at, often with `#note_<id>`.
    pub url: String,
    pub created: i64,
}

/// Who approved a merge request, how many approvals it needs, and where the reviewer stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Approvals {
    /// Names of those who approved.
    pub by: Vec<String>,
    /// Approvals the rules require (0 when there are no rules).
    pub required: u32,
    /// Approvals still missing.
    pub left: u32,
    /// The token's user approved it.
    pub mine: bool,
    /// The token's user may approve it (not the author on a project that forbids that, …).
    pub can: bool,
}

/// One push of the merge request's branch, as GitLab keeps it: the commit it ended on, the commit
/// the merge request was based on then, and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Push {
    /// GitLab's id of the version it made (a link to two versions compared names it, #162); 0 on
    /// GitHub, which has none.
    pub id: u64,
    pub head: String,
    pub base: String,
    pub created: i64,
}

/// One reaction on a note (#9): its id (to take one's own back), its GitLab name, who gave it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Award {
    pub id: u64,
    pub name: String,
    pub user: String,
}

/// A label of a request, with its colour as 0xRRGGBB when the project gives one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Label {
    pub name: String,
    pub color: Option<u32>,
}

/// What a request's Overview says beyond its list row (#8, #73).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Details {
    pub description: String,
    pub labels: Vec<Label>,
    /// The milestone's title and its due date (`YYYY-MM-DD`), if any.
    pub milestone: Option<(String, Option<String>)>,
    pub assignees: Vec<String>,
    /// The reviewers by name (GitHub: those still asked; who approved is in the approvals).
    pub reviewers: Vec<String>,
    /// The reviewers, by name, whose last word was to request changes.
    pub changes_requested: Vec<String>,
}

/// A milestone a new request can be put in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Milestone {
    /// GitLab's id, GitHub's number: what creating a request takes.
    pub id: u64,
    pub title: String,
    pub due: Option<String>,
}

/// Where a comment goes in a merge request's diff.
pub struct Place<'a> {
    pub path: &'a str,
    pub old_path: &'a str,
    pub new_line: Option<u32>,
    pub old_line: Option<u32>,
    /// A comment on several lines: the first and the last, each with both counters
    /// (`FileDiff::position`). The last is the line above.
    pub range: Option<(DiffLine, DiffLine)>,
}

/// What a new request asks for besides its branches, title and description.
#[derive(Clone, Debug, Default)]
pub struct NewOptions {
    pub draft: bool,
    pub remove_source: bool,
    pub squash: bool,
    /// Usernames to ask for a review.
    pub reviewers: Vec<String>,
    /// Label names, and the milestone's id (GitHub: number).
    pub labels: Vec<String>,
    pub milestone: Option<u64>,
}

/// A review's verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Comment,
    Approve,
    RequestChanges,
}

/// What the host says of merging the request, read with it (`merge::Host` before #165's stage 3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeState {
    pub github: bool,
    /// GitLab's `detailed_merge_status`, GitHub's `mergeable_state`.
    pub status: String,
    /// The token's user may merge into the target.
    pub can: bool,
    pub conflicts: bool,
    /// Commits the target has that the branch has not (GitLab's `diverged_commits_count`).
    pub behind: u32,
    /// The project wants the pipeline to pass first, and every discussion resolved.
    pub pipeline_must_pass: bool,
    pub discussions_must_resolve: bool,
    /// Set to merge when the pipeline succeeds already.
    pub auto_set: bool,
    /// Merging when the pipeline succeeds can be set: GitLab always, GitHub where the
    /// repository has auto-merge on.
    pub auto_allowed: bool,
    pub squash: Squash,
    /// Delete the source branch after, as the request or the project has it.
    pub delete_branch: bool,
    /// GitHub deletes the branch itself after a merge (`delete_branch_on_merge`).
    pub deletes_itself: bool,
    /// GitHub: the ways the repository allows, its default first.
    pub methods: Vec<Method>,
    /// GitHub's node id of the pull request, for auto-merge's mutations.
    pub node: String,
}

/// Squashing the request's commits into one, as the project allows it (GitLab's `squash_option`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Squash {
    #[default]
    Never,
    Always,
    /// The merger chooses; the project's default.
    Choose(bool),
}

/// How GitHub puts a pull request into its base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Merge,
    Squash,
    Rebase,
}

impl Method {
    pub fn label(self) -> &'static str {
        match self {
            Method::Merge => "Merge commit",
            Method::Squash => "Squash",
            Method::Rebase => "Rebase",
        }
    }

    /// The REST API's `merge_method`.
    pub fn key(self) -> &'static str {
        match self {
            Method::Merge => "merge",
            Method::Squash => "squash",
            Method::Rebase => "rebase",
        }
    }
}

/// What a merge sends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Merge {
    /// The head that was reviewed: the host refuses when the branch moved on since.
    pub sha: String,
    pub squash: bool,
    pub delete_branch: bool,
    /// GitHub's way.
    pub method: Method,
    /// The commit's message: its first line is GitHub's title.
    pub message: String,
    /// Set to merge when the pipeline succeeds, rather than now.
    pub when_pipeline_succeeds: bool,
}

impl Merge {
    /// The card's start: the project's defaults, the title and the reference as the message.
    pub fn defaults(
        host: &MergeState,
        sha: &str,
        message: String,
        when_pipeline_succeeds: bool,
    ) -> Self {
        let method = host.methods.first().copied().unwrap_or(Method::Merge);
        Merge {
            sha: sha.to_owned(),
            squash: match host.squash {
                Squash::Never => false,
                Squash::Always => true,
                Squash::Choose(on) => on,
            },
            delete_branch: host.delete_branch,
            method,
            message,
            when_pipeline_succeeds,
        }
    }
}

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
