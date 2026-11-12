//! Merge requests as the hosts give them (#165): a request, its discussions and notes, pending comments, approvals,
//! pipelines, pushes, reactions, what stands in a merge's way and what a merge sends — the plain types the hosts'
//! contract answers in and the requests' speaks (`artoo_hosts_api` and `artoo_requests_api` show them).

use crate::git::{BranchRef, DiffLine, RefKind, Repo};

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

/// A request's size: files, lines added, lines removed, commits.
pub type Size = (usize, usize, usize, usize);

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
    /// The reviewers' names as the host gives them, beside `reviewers` in their order; empty where it gives none.
    pub reviewer_names: Vec<String>,
    /// What the author wrote about it, as the list gives it: the root page's preview (#72).
    pub description: String,
    pub state: MrState,
    /// What it was merged as (the merge or squash commit): its first parent is the target as it
    /// was, so the diff is the request's, not what the target became since.
    pub merged_with: Option<String>,
    /// The host says the branch conflicts with its target (GitLab's `has_conflicts`; GitHub's list does not say).
    pub conflicts: bool,
}

impl Mr {
    /// The name the host gives reviewer `login`, when it gives one (#189).
    pub fn reviewer_name(&self, login: &str) -> Option<&str> {
        (self.reviewers.iter().zip(&self.reviewer_names))
            .find(|(l, name)| *l == login && !name.is_empty())
            .map(|(_, name)| name.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Note {
    /// GitLab's id: a link's `#note_<id>` points at it.
    pub id: u64,
    pub author: String,
    /// The author's username, to tell one's own notes.
    pub login: String,
    pub body: String,
    pub created: i64,
    /// Changed after it was written, as the host says (#199): its time reads `20 min ago · edited`.
    #[serde(default)]
    pub edited: bool,
}

/// A discussion on a merge request: on a line of a file, or on the request as a whole.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Approvals {
    /// Names of those who approved.
    pub by: Vec<String>,
    /// Approvals the rules require (0 when there are no rules).
    pub required: u32,
    /// Approvals still missing.
    pub left: u32,
    /// The token's user approved it.
    pub mine: bool,
    /// When the token's user approved it, when the host says: a push after it gives the turn back (#189).
    pub mine_at: Option<i64>,
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

/// A request's base and head as its host compares it (GitLab: the latest version's `diff_refs`;
/// GitHub: the base branch's and the head's commits); `None` where the host gave none.
pub type HostSides = (Option<git2::Oid>, Option<git2::Oid>);

/// How a request is compared once its host's sides are known (#145).
#[derive(Debug, PartialEq, Eq)]
pub enum Sides {
    /// The sides read from this clone already give the host's diff.
    Same,
    /// The host's base and head, both here: the request is compared from them.
    Host(git2::Oid, git2::Oid),
    /// The host names a commit this clone lacks — the base, the head or both —, to fetch first.
    Missing { base: bool, head: bool },
}

/// The host's sides against the ones read from this clone, `here`: the target's tip (a merged
/// request's merge commit's first parent) and the source branch's. A target not fetched lately
/// starts the compare at an old merge base, and every target commit after it shows as the
/// request's; a fast-forward merge puts the head in the target, and nothing shows.
pub fn sides(repo: &Repo, host: HostSides, here: (git2::Oid, git2::Oid)) -> Sides {
    let lacks = |id: Option<git2::Oid>| id.is_some_and(|id| !repo.has_commit(id));
    let (base, head) = (lacks(host.0), lacks(host.1));
    if base || head {
        return Sides::Missing { base, head };
    }
    let (base, head) = (host.0.unwrap_or(here.0), host.1.unwrap_or(here.1));
    if head == here.1 && repo.start_of(base, head) == repo.start_of(here.0, here.1) {
        Sides::Same
    } else {
        Sides::Host(base, head)
    }
}

/// The branch `branch` names as the host does: `feature-x`, not `origin/feature-x`.
pub fn source_of(branch: &BranchRef) -> String {
    match branch.kind {
        RefKind::Remote => branch
            .name
            .split_once('/')
            .map_or(branch.name.clone(), |(_, n)| n.to_owned()),
        RefKind::Local | RefKind::Request => branch.name.clone(),
    }
}
