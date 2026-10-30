//! What the tab keeps of merge requests: the lists — open, merged or closed, by scope —, the request open with its
//! discussions, pending comments, approvals, pipeline and versions, what its host compares it from, and the changes
//! sent to it not yet read back. The tab reads and draws them.

use crate::group::Group;
use artoo_core::git::{Repo, Version};
use artoo_hosts_api::{self as mr, Draft, Mr, Pipeline, Thread};
use gpui::Task;

/// Which requests the list shows: All · To review · Mine in Merge requests' bar (#70, #72).
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    All,
    /// The token's user is asked to review.
    ReviewMe,
    /// The token's user wrote it.
    Mine,
}

/// The merged or the closed requests (#114): each scope's own, as the host picks them (#169).
#[derive(Default)]
pub struct Past {
    /// Which they are; Opened shows the open list.
    pub of: mr::MrState,
    /// All's, To review's and Mine's.
    pub scopes: [Pages; 3],
}

impl Past {
    pub fn of_scope(&self, scope: Scope) -> &Pages {
        &self.scopes[scope as usize]
    }

    pub fn of_scope_mut(&mut self, scope: Scope) -> &mut Pages {
        &mut self.scopes[scope as usize]
    }
}

/// One scope's merged or closed requests, read a page at a time as the list is scrolled.
#[derive(Default)]
pub struct Pages {
    pub list: Vec<Mr>,
    /// Where the next page starts (none: the first), and whether one follows.
    pub at: Option<String>,
    pub more: bool,
    /// How many there are in all, where the host says.
    pub total: Option<u32>,
    pub loading: bool,
    pub error: Option<String>,
    pub task: Option<Task<()>>,
}

/// What the requests tell the tab after a read of their own (`reading`).
pub enum Event {
    /// The open list was read, or could not be.
    Listed,
    /// Requests gone from the open list were read.
    Left,
    /// Request `iid`, opened, was read (`read`), or could not be.
    Opened { iid: u64, read: bool },
    /// The open request `iid`'s discussions, pending comments and approvals were read again: what
    /// shows is made again on them.
    Read(u64),
    /// Request `iid`'s versions were read: its diff follows them.
    Versions(u64),
    /// The lines of the outdated discussions were followed to the latest version.
    Histories,
    /// The reactions on the open request's notes were read.
    Awards,
    /// The host merged request `iid`, or set it to merge when the pipeline succeeds (`merge`).
    Merged {
        iid: u64,
        merge: crate::merge::Merge,
    },
    /// The host created request `iid` from branch `source`.
    Created { iid: u64, source: String },
    /// A call to the host was refused, and why.
    Failed(String),
}

impl gpui::EventEmitter<Event> for Requests {}

/// What a view of the requests asks the tab (`Acts`).
pub enum Act {
    /// `a`: approve the request open, or take one's approval back.
    ToggleApproval,
    /// The list of what is in the way of a merge, at a window position.
    MergeReasons(gpui::Point<gpui::Pixels>),
    /// The card before a merge goes.
    OpenMerge,
    /// The host puts the request's commits on top of its target.
    Rebase,
    /// Merging when the pipeline succeeds is cancelled.
    CancelAutoMerge,
    /// Your reaction `name` on note `note`, added or taken back.
    React { note: u64, name: String },
    /// ☺+ opened on note `note`.
    OpenGrid(u64),
    /// ☺+ closed, when it is open on note `note`.
    CloseGrid(u64),
    /// A note for your agent on discussion `thread` (#50 G).
    ThreadNote(usize),
    /// A reply to discussion `thread` with `login` already mentioned.
    ReplyMentioning { thread: usize, login: String },
    /// Outdated discussion `ix`'s version compared with the latest.
    CompareSince(usize),
    /// The diff shows line `line` of file `path` (`old`: the removed side).
    JumpToLine { path: String, line: u32, old: bool },
    /// Submit review's card.
    OpenSubmit,
    /// Pending comment `id` published alone (#138).
    SendNow(u64),
    /// Submit review's card sends the review.
    RunSubmit,
    /// The New merge request form creates the request, or opens the one there is.
    CreateGo,
    /// Templates ▾ chose template `ix` for the description.
    UseTemplate(usize),
    /// From's list chose a branch.
    PickSource(String),
    /// Into's list chose a branch.
    PickTarget(String),
    /// Merge's card sends the merge.
    RunMerge,
    /// Merge was refused for a push after the version on the page: the card goes, the new one
    /// comes.
    ShowNewVersion,
}

/// How the tab answers what the requests' views ask.
pub type Acts = std::rc::Rc<dyn Fn(Act, &mut gpui::App)>;

#[derive(Default)]
pub struct Requests {
    pub scope: Scope,
    /// The project's members, for `@` completion (#51).
    pub members: Vec<(String, String)>,
    /// Members a mention does not reach (a private project's guests): "not notified" (#51).
    pub unreached: Vec<String>,
    pub available: bool,
    /// A GitLab project without a token: the rail offers to connect instead of listing.
    pub connectable: bool,
    pub list: Vec<Mr>,
    /// The state the list shows (Open ▾, #114), and the merged or closed read so far.
    pub view: mr::MrState,
    pub past: Past,
    /// Requests open in the menu that left the open list: what became of them (merged, closed),
    /// kept until closed there.
    pub left: Vec<Mr>,
    /// Requests whose head was fetched, its branch not being here: one try each.
    pub heads_fetched: std::collections::HashSet<u64>,
    /// Requests whose base or head the host compares from was fetched, not being here: one try
    /// each (#145).
    pub sides_fetched: std::collections::HashSet<u64>,
    /// Requests read one by one (gone from the open list, or a link's): one try each.
    pub tried: std::collections::HashSet<u64>,
    pub loading: bool,
    pub error: Option<String>,
    /// When the list last came in: an error or being offline says how old the list is (#72).
    pub listed_at: Option<i64>,
    /// The request of the open branch, when it is one's source branch.
    pub current: Option<u64>,
    /// The open request and the group it was in when it was opened: ⌥↓ goes on from its place
    /// there, not from Reviewed, where reviewing it to the end moved it (#136).
    pub walk: Option<(u64, Group)>,
    /// Its discussions are being read: Discussions shimmers instead of saying there are none.
    pub reading: bool,
    /// What shows of its discussions, pending comments and approvals: `base` with the changes
    /// still `waiting` on it (#139).
    pub threads: Vec<Thread>,
    /// One's own pending comments on the current request.
    pub drafts: Vec<Draft>,
    /// The request's newest pipeline.
    pub pipeline: Option<Pipeline>,
    /// Who approved the request, and where the reviewer stands.
    pub approvals: Option<mr::Approvals>,
    /// What the host says of merging an open one (#164), and the call about it on its way: its
    /// word in Merge's place.
    pub merge: Option<mr::MergeState>,
    pub merge_busy: Option<&'static str>,
    pub merge_task: Option<Task<()>>,
    /// Merge's card, open.
    pub merging: Option<crate::merging::MergeCard>,
    /// Submit review's card, while it is open.
    pub submitting: Option<crate::submit::SubmitCard>,
    /// The New merge request form, while it is open.
    pub creating: Option<crate::create::NewRequest>,
    /// The host's last answer for them, and the tick the read that gave it began at.
    pub base: crate::waiting::Base,
    pub from: u64,
    /// The changes sent to the host and not yet read back, on any request (#139).
    pub waiting: crate::waiting::Waiting,
    pub reload_task: Option<Task<()>>,
    /// Its description, labels, milestone and assignees, for its Overview (#8, #73).
    pub details: Option<mr::Details>,
    /// The files of a request's whole diff when it was last loaded: its Changes tab's count, on a
    /// commit of it too (#73).
    pub files: Option<(u64, usize)>,
    /// Discussions written on an earlier version: what became of their line, by thread id (#46).
    pub histories: std::collections::HashMap<String, crate::outdated::LineHistory>,
    pub history_task: Option<Task<()>>,
    /// The reactions on its notes, by note id (#9).
    pub awards: crate::reactions::Awards,
    pub award_task: Option<Task<()>>,
    /// ☺+, open on a note.
    pub reaction_grid: Option<crate::reactions::Grid>,
    /// The pushes GitLab kept for a request, as versions of this repository.
    pub versions: Option<(u64, Vec<Version>)>,
    /// The base and head the host compares that request from — its latest push's, as given, here
    /// or not (#145).
    pub host: Option<(u64, HostSides)>,
    /// GitLab's id of each of that request's versions, by its head: a link to two of them compared
    /// names the newer by it (#162).
    pub version_ids: Option<(u64, Vec<(git2::Oid, u64)>)>,
    pub version_task: Option<Task<()>>,
    /// The token's username: one's own notes are the ones it wrote, and can be edited.
    pub login: Option<String>,
    pub task: Option<Task<()>>,
    pub thread_task: Option<Task<()>>,
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

impl Requests {
    /// Request `iid` wherever it was read: the open list, the merged or closed, or the ones that
    /// left the open list while open in the menu.
    pub fn find(&self, iid: u64) -> Option<&Mr> {
        self.list
            .iter()
            .chain(self.past.scopes.iter().flat_map(|p| &p.list))
            .chain(&self.left)
            .find(|m| m.iid == iid)
    }

    /// The discussions on lines of file `path`.
    pub fn request_threads_in(&self, path: &str) -> Vec<(usize, bool, u32)> {
        self.threads
            .iter()
            .enumerate()
            .filter(|(_, t)| t.path.as_deref() == Some(path))
            .filter_map(|(ix, t)| {
                let (old, line) = self.thread_line_now(t)?;
                Some((ix, old, line))
            })
            .collect()
    }

    /// Where discussion `t` is drawn in the diff on screen: (old side, line).
    fn thread_line_now(&self, t: &Thread) -> Option<(bool, u32)> {
        // Written on an earlier version: drawn at its line now, or not at all (#46).
        if self.thread_is_old(t) {
            let h = self.histories.get(&t.id)?;
            return h.line_now.map(|n| (false, n));
        }
        match (t.new_line, t.old_line) {
            (Some(n), _) => Some((false, n)),
            (None, Some(o)) => Some((true, o)),
            _ => None,
        }
    }

    /// The discussion pending comment `d` replies to, when it is one of the request's.
    pub fn draft_thread(&self, d: &Draft) -> Option<usize> {
        let id = d.reply_to.as_deref()?;
        self.threads.iter().position(|t| t.id == id)
    }

    /// Where pending comment `d` is: its own place, or for a reply its discussion's (#166) —
    /// (path, new line, old line); the path alone for a file's, nothing for the request's.
    pub fn draft_place(&self, d: &Draft) -> Option<(String, Option<u32>, Option<u32>)> {
        match self.draft_thread(d).map(|ix| &self.threads[ix]) {
            Some(t) => Some((t.path.clone()?, t.new_line, t.old_line)),
            None => Some((d.path.clone()?, d.new_line, d.old_line)),
        }
    }

    /// Where pending comment `d` is, said in a list: `file.rs:12`, or why it has no line.
    pub fn draft_where(&self, d: &Draft) -> (String, Option<(String, u32)>, bool) {
        let place = self.draft_place(d);
        let target = place
            .as_ref()
            .and_then(|(p, new, old)| Some((p.clone(), new.or(*old)?)));
        let old = place.as_ref().is_some_and(|(_, new, _)| new.is_none());
        let said = match &target {
            Some((p, n)) => format!("{}:{n}", p.rsplit('/').next().unwrap_or(p)),
            // A reply whose discussion is not among the request's: not General, it was in one.
            None if d.reply_to.is_some() && self.draft_thread(d).is_none() => {
                "A reply to a discussion not here".to_owned()
            }
            None => "General".to_owned(),
        };
        (said, target, old)
    }

    /// The pending comments of file `path`: (index, old side, line).
    pub fn drafts_in(&self, path: &str) -> Vec<(usize, bool, u32)> {
        self.drafts
            .iter()
            .enumerate()
            .filter_map(|(ix, d)| {
                // A reply is drawn under its discussion, wherever that is now (#166).
                if let Some(t) = self.draft_thread(d).map(|t| &self.threads[t]) {
                    let (old, line) = self
                        .thread_line_now(t)
                        .filter(|_| t.path.as_deref() == Some(path))?;
                    return Some((ix, old, line));
                }
                if d.path.as_deref() != Some(path) {
                    return None;
                }
                match (d.new_line, d.old_line) {
                    (Some(n), _) => Some((ix, false, n)),
                    (None, Some(o)) => Some((ix, true, o)),
                    _ => None,
                }
            })
            .collect()
    }

    /// Whether `m` is one the scope lists, by the token's user.
    pub fn in_scope(&self, m: &Mr, scope: Scope) -> bool {
        let me = self.login.as_deref().unwrap_or_default();
        match scope {
            Scope::All => true,
            Scope::ReviewMe => !me.is_empty() && m.reviewers.iter().any(|r| r == me),
            Scope::Mine => !me.is_empty() && m.author_login == me,
        }
    }

    /// Whether the token's user approved the current request already.
    pub fn approved_already(&self) -> bool {
        self.approvals.as_ref().is_some_and(|a| a.mine)
    }
}
