//! What the tab keeps of merge requests: the lists — open, merged or closed, by scope —, the request open with its
//! discussions, pending comments, approvals, pipeline and versions, what its host compares it from, and the changes
//! sent to it not yet read back. The tab reads and draws them.

use crate::group::Group;
use artoo_core::git::{BranchRef, RefKind, Version};
use artoo_hosts_api::{self as mr, Draft, Mr, Pipeline, Thread};
use artoo_requests_api::{
    Answered, Change, DraftLook, Drafted, InboxFilter, InboxRow, Moved, OnDraft, OnInbox, OnThread,
    Pair, Since, SubmitLook, ThreadLook,
};
use artoo_ui_kit::markdown::Selecting;
use gpui::KeyDownEvent;
use gpui::{AnyElement, IntoElement};
use gpui::{App, Entity, Task, WeakEntity};
use std::rc::Rc;

pub use artoo_requests_api::Scope;

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

pub use artoo_requests_api::Event;

impl gpui::EventEmitter<Event> for Requests {}

pub use artoo_requests_api::{Act, Acts};

#[derive(Default)]
pub struct Requests {
    /// The repository open and its remote's web address: what is read, and the host asked.
    pub root: Option<std::path::PathBuf>,
    pub web: Option<artoo_core::git::WebRemote>,
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
    /// The list's rows: their previews, its scroll, the groups folded, the requests opened.
    pub rows: crate::list::Rows,
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
    /// A comparison opened from an outdated discussion, with the versions that changed its line.
    pub since: Option<crate::outdated::Since>,
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

pub use artoo_core::mr::HostSides;

impl Requests {
    /// The remote's host, when the repository has one.
    pub fn host(&self, cx: &App) -> Option<std::sync::Arc<dyn artoo_hosts_api::Host>> {
        Some(artoo_hosts_api::get(cx).of(self.web.as_ref()?))
    }

    /// The request on branch `b`: a local or remote branch's, by the branch it is from; a request's own ref's, by
    /// its number.
    pub fn of_branch(&self, b: &BranchRef) -> Option<&Mr> {
        let name = match b.kind {
            RefKind::Remote => b.name.split_once('/').map_or(b.name.as_str(), |(_, n)| n),
            RefKind::Local => b.name.as_str(),
            RefKind::Request => {
                let iid = b.name.rsplit('/').next()?.parse().ok()?;
                return self.find(iid);
            }
        };
        self.list.iter().find(|m| m.source == name)
    }

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

/// The tab's requests as other features ask them (`artoo_requests_api::Requests`, a service); a click in what it
/// draws asks the tab's acts in `services`.
pub(crate) fn handle(
    requests: &Entity<Requests>,
    services: &Rc<artoo_shell_api::Services>,
) -> Rc<dyn artoo_requests_api::Requests> {
    Rc::new(Handle(requests.downgrade(), Rc::downgrade(services)))
}

/// How the requests' views ask: the past page read again is theirs, the rest the tab's (`tab`).
pub(crate) fn acts(requests: &Entity<Requests>, tab: artoo_requests_api::TabActs) -> Acts {
    let (tab, requests) = (tab.0, requests.downgrade());
    Rc::new(move |act, cx| match act {
        Act::RetryPast => {
            if let Some(requests) = requests.upgrade()
                && let Some(host) = requests.read(cx).host(cx)
            {
                requests.update(cx, |rq, cx| rq.retry_past_page(host, cx));
            }
        }
        act => tab(act, cx),
    })
}

/// Weak: the services hold it.
struct Handle(
    WeakEntity<Requests>,
    std::rc::Weak<artoo_shell_api::Services>,
);

impl Handle {
    /// Runs `f` on the requests with their host, when they have one.
    fn with_host(
        &self,
        cx: &mut App,
        f: impl FnOnce(
            &mut Requests,
            std::sync::Arc<dyn artoo_hosts_api::Host>,
            &mut gpui::Context<Requests>,
        ),
    ) {
        let Some(requests) = self.0.upgrade() else {
            return;
        };
        let Some(host) = requests.read(cx).host(cx) else {
            return;
        };
        requests.update(cx, |rq, cx| f(rq, host, cx));
    }

    /// Runs `f` on the requests.
    fn update<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Requests, &mut gpui::Context<Requests>) -> R,
    ) -> Option<R> {
        Some(self.0.upgrade()?.update(cx, f))
    }

    /// The Inbox's notices, from the tab's services.
    fn notices(&self) -> Option<Entity<crate::notices::Notices>> {
        self.1.upgrade()?.get()
    }

    /// How far you reviewed request `m` (the review's word).
    fn seen(&self, m: &Mr, cx: &App) -> artoo_review_api::Seen {
        self.review().map_or(artoo_review_api::Seen::Never, |r| {
            r.request_seen(m.iid, git2::Oid::from_str(&m.head).ok(), cx)
        })
    }

    /// The tab's review and its branches, from its services.
    fn review(&self) -> Option<Rc<dyn artoo_review_api::Review>> {
        self.1.upgrade()?.get()
    }

    fn branch_list(&self, cx: &App) -> std::sync::Arc<Vec<BranchRef>> {
        self.1
            .upgrade()
            .and_then(|s| s.get::<Rc<dyn artoo_branches_api::Branches>>())
            .map(|b| b.list(cx))
            .unwrap_or_default()
    }

    /// The Inbox's view, from the tab's services.
    fn inbox(&self) -> Option<Entity<crate::inbox::InboxView>> {
        self.1.upgrade()?.get()
    }

    /// Reads the notices.
    fn read_notices_with<R>(
        &self,
        cx: &App,
        f: impl FnOnce(&crate::notices::Notices) -> R,
    ) -> Option<R> {
        Some(f(self.notices()?.read(cx)))
    }

    /// How a view's click asks the tab.
    fn acts(&self) -> Acts {
        self.1
            .upgrade()
            .and_then(|s| s.get::<Acts>())
            .unwrap_or_else(|| Rc::new(|_, _| {}))
    }
}

impl artoo_requests_api::Requests for Handle {
    fn of_branch(&self, branch: &BranchRef, cx: &App) -> Option<(u64, String)> {
        let requests = self.0.upgrade()?;
        let m = requests.read(cx).of_branch(branch)?;
        Some((m.iid, m.title.clone()))
    }

    fn current(&self, cx: &App) -> Option<u64> {
        self.0.upgrade()?.read(cx).current
    }

    fn listing(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|r| r.read(cx).loading)
    }

    fn connectable(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|r| r.read(cx).connectable)
    }

    fn unlisted(&self, numbers: &[u64], cx: &App) -> Vec<u64> {
        let Some(requests) = self.0.upgrade() else {
            return Vec::new();
        };
        let rq = requests.read(cx);
        if rq.error.is_some() {
            return Vec::new();
        }
        numbers
            .iter()
            .copied()
            .filter(|n| rq.find(*n).is_none())
            .collect()
    }

    fn versions(&self, cx: &App) -> Option<(u64, Vec<artoo_core::git::Version>)> {
        self.0.upgrade()?.read(cx).versions.clone()
    }

    fn back_to_list(&self, number: u64, cx: &mut App) {
        if let Some(requests) = self.0.upgrade() {
            requests.update(cx, |rq, _| rq.rows.picked = Some(number));
            requests.read(cx).rows.reveal.set(true);
        }
    }

    fn find(&self, number: u64, cx: &App) -> Option<Mr> {
        self.0.upgrade()?.read(cx).find(number).cloned()
    }

    fn list(&self, cx: &App) -> Vec<Mr> {
        self.0
            .upgrade()
            .map(|r| r.read(cx).list.clone())
            .unwrap_or_default()
    }

    fn available(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|r| r.read(cx).available)
    }

    fn login(&self, cx: &App) -> Option<String> {
        self.0.upgrade()?.read(cx).login.clone()
    }

    fn members(&self, cx: &App) -> Vec<(String, String)> {
        self.0
            .upgrade()
            .map(|r| r.read(cx).members.clone())
            .unwrap_or_default()
    }

    fn approvals(&self, cx: &App) -> Option<mr::Approvals> {
        self.0.upgrade()?.read(cx).approvals.clone()
    }

    fn threads(&self, cx: &App) -> Vec<Thread> {
        self.0
            .upgrade()
            .map(|r| r.read(cx).threads.clone())
            .unwrap_or_default()
    }

    fn thread(&self, ix: usize, cx: &App) -> Option<Thread> {
        self.0.upgrade()?.read(cx).threads.get(ix).cloned()
    }

    fn drafts(&self, cx: &App) -> Vec<Draft> {
        self.0
            .upgrade()
            .map(|r| r.read(cx).drafts.clone())
            .unwrap_or_default()
    }

    fn draft_place(&self, ix: usize, cx: &App) -> Option<(String, Option<u32>, Option<u32>)> {
        let requests = self.0.upgrade()?;
        let rq = requests.read(cx);
        rq.draft_place(rq.drafts.get(ix)?)
    }

    fn threads_in(&self, path: &str, cx: &App) -> Vec<(usize, bool, u32)> {
        self.0
            .upgrade()
            .map(|r| r.read(cx).request_threads_in(path))
            .unwrap_or_default()
    }

    fn request_tip(&self, number: u64, cx: &App) -> Option<String> {
        self.0.upgrade()?.read(cx).request_tip(number)
    }

    fn version_id(&self, number: u64, head: git2::Oid, cx: &App) -> Option<u64> {
        let requests = self.0.upgrade()?;
        let (of, ids) = requests.read(cx).version_ids.as_ref()?;
        (*of == number)
            .then(|| ids.iter().find(|(h, _)| *h == head).map(|(_, id)| *id))
            .flatten()
    }

    fn tried(&self, number: u64, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|r| r.read(cx).tried.contains(&number))
    }

    fn count_in(&self, scope: Scope, cx: &App) -> usize {
        self.0.upgrade().map_or(0, |r| {
            let rq = r.read(cx);
            rq.list.iter().filter(|m| rq.in_scope(m, scope)).count()
        })
    }

    fn grid_open(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|r| r.read(cx).reaction_grid.is_some())
    }

    fn merge_card_open(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|r| r.read(cx).merging.is_some())
    }

    fn completion(
        &self,
        text: &str,
        caret: usize,
        closed: Option<usize>,
        cx: &App,
    ) -> Option<(usize, Vec<artoo_ui_kit::complete::Choice>)> {
        self.0.upgrade()?.read(cx).completion(text, caret, closed)
    }

    fn files(&self, number: u64, cx: &App) -> Option<usize> {
        let (of, n) = self.0.upgrade()?.read(cx).files?;
        (of == number).then_some(n)
    }

    fn set_files(&self, files: Option<(u64, usize)>, cx: &mut App) {
        if let Some(requests) = self.0.upgrade() {
            requests.update(cx, |rq, _| rq.files = files);
        }
    }

    fn picked_row(&self, cx: &App) -> Option<u64> {
        self.0.upgrade()?.read(cx).rows.picked
    }

    fn approval_chip(&self, cx: &mut App) -> Option<gpui::AnyElement> {
        let acts = self.acts();
        let chip = self.0.upgrade()?.read(cx).render_approval_chip(acts)?;
        Some(gpui::IntoElement::into_any_element(chip))
    }

    fn merge_control(&self, compact: bool, github: bool, cx: &mut App) -> Option<gpui::AnyElement> {
        let acts = self.acts();
        self.0
            .upgrade()?
            .read(cx)
            .render_merge_control(compact, github, acts)
    }

    fn set_repository(
        &self,
        root: Option<std::path::PathBuf>,
        web: Option<artoo_core::git::WebRemote>,
        cx: &mut App,
    ) {
        self.update(cx, |rq, _| (rq.root, rq.web) = (root, web));
    }

    fn reset(&self, cx: &mut App) {
        self.update(cx, |rq, _| {
            let (root, web) = (rq.root.take(), rq.web.take());
            *rq = Requests::default();
            (rq.root, rq.web) = (root, web);
        });
    }

    fn set_token(&self, available: bool, connectable: bool, loading: bool, cx: &mut App) {
        self.update(cx, |rq, _| {
            rq.available = available;
            rq.connectable = connectable;
            rq.loading = loading;
        });
    }

    fn clear_list(&self, cx: &mut App) {
        self.update(cx, |rq, _| {
            rq.list.clear();
            rq.error = None;
        });
    }

    fn read_list(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.read_list(host, cx));
    }

    fn set_view(&self, state: mr::MrState, cx: &mut App) {
        let reads = self.update(cx, |rq, _| {
            rq.view = state;
            rq.rows.picked = None;
            if state == mr::MrState::Opened || rq.past.of == state {
                return false;
            }
            // Every scope's first page at once: the chips count them all.
            let mut past = crate::requests::Past {
                of: state,
                ..Default::default()
            };
            for pages in &mut past.scopes {
                pages.more = true;
            }
            rq.past = past;
            true
        });
        if reads == Some(true) {
            for scope in [Scope::All, Scope::ReviewMe, Scope::Mine] {
                self.load_past_page(scope, cx);
            }
        }
    }

    fn scope(&self, cx: &App) -> Scope {
        self.0
            .upgrade()
            .map(|r| r.read(cx).scope)
            .unwrap_or_default()
    }

    fn load_past_page(&self, scope: Scope, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.load_past_page(scope, host, cx));
    }

    fn retry_past_page(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.retry_past_page(host, cx));
    }

    fn read_left(&self, numbers: Vec<u64>, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.read_left(numbers, host, cx));
    }

    fn reload_threads(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.reload_threads(host, cx));
    }

    fn read_open(&self, number: u64, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.read_open(number, host, cx));
    }

    fn load_versions(&self, number: u64, cx: &mut App) {
        self.update(cx, |rq, _| {
            rq.versions = None;
            rq.host = None;
        });
        self.with_host(cx, |rq, host, cx| {
            if let Some(root) = rq.root.clone() {
                rq.load_versions(number, host, root, cx);
            }
        });
    }

    fn compute_histories(&self, cx: &mut App) {
        self.update(cx, |rq, cx| {
            if let Some(root) = rq.root.clone() {
                rq.compute_histories(root, cx);
            }
        });
    }

    fn load_awards(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.load_awards(host, cx));
    }

    fn toggle_reaction(&self, note: u64, name: &str, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.toggle_reaction(note, name, host, cx));
    }

    fn read_preview(&self, request: &Mr, viewed: Option<(git2::Oid, git2::Oid)>, cx: &mut App) {
        let settings = self
            .1
            .upgrade()
            .and_then(|s| s.get::<Rc<dyn artoo_review_api::Review>>())
            .map(|review| review.settings(cx))
            .unwrap_or_default();
        self.with_host(cx, |rq, host, cx| {
            if let Some(root) = rq.root.clone() {
                rq.read_preview(request, viewed, host, root, settings, cx);
            }
        });
    }

    fn reload_merge(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.reload_merge(host, cx));
    }

    fn run_merge(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| rq.run_merge(host, cx));
    }

    fn rebase(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| {
            rq.merge_call("Rebasing…", host, |host, iid, _| host.rebase(iid), cx)
        });
    }

    fn cancel_auto_merge(&self, cx: &mut App) {
        self.with_host(cx, |rq, host, cx| {
            rq.merge_call(
                "Cancelling…",
                host,
                |host, iid, node| host.cancel_auto_merge(iid, node),
                cx,
            )
        });
    }

    fn merged(&self, number: u64, merge: &mr::Merge, github: bool, cx: &mut App) -> Option<String> {
        self.update(cx, |rq, _| {
            let here = rq.current == Some(number);
            let reference = crate::merging::reference(number, github);
            if merge.when_pipeline_succeeds {
                if let Some(host) = rq.merge.as_mut().filter(|_| here) {
                    host.auto_set = true;
                }
                return Some(format!("{reference} merges when the pipeline succeeds"));
            }
            let deletes = here && rq.merge.as_ref().is_some_and(|h| h.deletes_itself);
            let said = rq.find(number).map(|m| {
                let deleted = match merge.delete_branch || deletes {
                    true => format!(" · {} deleted", m.source),
                    false => String::new(),
                };
                format!("{reference} merged into {}{deleted}", m.target)
            });
            if here {
                rq.merge = None;
            }
            said
        })
        .flatten()
    }

    fn open_merge_card(&self, when: bool, github: bool, cx: &mut App) -> bool {
        self.update(cx, |rq, _| rq.open_merge_card(when, github))
            .unwrap_or(false)
    }

    fn merge_key(&self, event: &gpui::KeyDownEvent, cx: &mut App) -> bool {
        self.update(cx, |rq, cx| rq.merge_key(event, cx))
            .unwrap_or(false)
    }

    fn drop_merge(&self, cx: &mut App) {
        self.update(cx, |rq, _| {
            rq.merging = None;
            rq.merge = None;
        });
    }

    fn merge_stand(&self, cx: &App) -> Option<artoo_requests_api::Stand> {
        self.0.upgrade()?.read(cx).merge_stand()
    }

    fn pipeline_url(&self, cx: &App) -> Option<String> {
        Some(self.0.upgrade()?.read(cx).pipeline.as_ref()?.url.clone())
    }

    fn merge_card(
        &self,
        look: artoo_requests_api::CardLook,
        cx: &mut App,
    ) -> Option<gpui::AnyElement> {
        let acts = self.acts();
        self.update(cx, |rq, cx| {
            rq.render_merge_card(look, acts, cx)
                .map(gpui::IntoElement::into_any_element)
        })
        .flatten()
    }

    fn open_grid(&self, note: u64, cx: &mut App) {
        self.update(cx, |rq, _| rq.open_reaction_grid(note));
    }

    fn close_grid(&self, cx: &mut App) {
        self.update(cx, |rq, _| rq.close_reaction_grid());
    }

    fn grid_note(&self, cx: &App) -> Option<u64> {
        self.0.upgrade()?.read(cx).grid_note()
    }

    fn reaction_key(
        &self,
        event: &gpui::KeyDownEvent,
        github: bool,
        cx: &mut App,
    ) -> Option<artoo_requests_api::GridKey> {
        self.update(cx, |rq, cx| rq.reaction_key(event, github, cx))
            .flatten()
    }

    fn submit_create(&self, cx: &mut App) -> Option<u64> {
        let requests = self.0.upgrade()?;
        let host = requests.read(cx).host(cx)?;
        requests.update(cx, |rq, cx| rq.submit_create(host, cx))
    }

    fn send(&self, number: u64, change: Change, answered: Answered, cx: &mut App) -> bool {
        let notices = self.notices();
        let mut sent = false;
        self.with_host(cx, |rq, host, cx| {
            let ops = rq.ops_of(number, change);
            rq.send_ops(number, ops, host, notices, answered, cx);
            sent = true;
        });
        sent
    }

    fn submit(&self, answered: Answered, cx: &mut App) -> bool {
        let notices = self.notices();
        let mut sent = false;
        self.with_host(cx, |rq, host, cx| {
            let Some(crate::submit::Submission {
                iid,
                ops,
                work,
                card,
            }) = rq.submission(cx)
            else {
                return;
            };
            // Refused, the card comes back saying why, unless another is open or another request shows.
            let this = cx.weak_entity();
            let answered: Answered = Box::new(move |result, cx| {
                let result = match result {
                    Err(Some(why)) => {
                        let back = this.update(cx, |rq, cx| {
                            cx.notify();
                            rq.submit_refused(iid, card, why.clone())
                        });
                        Err((!back.unwrap_or(false)).then_some(why))
                    }
                    other => other,
                };
                answered(result, cx);
            });
            rq.send_ops(iid, (ops, work), host, notices, answered, cx);
            sent = true;
        });
        sent
    }

    fn show_waiting(&self, number: u64, me: &str, cx: &mut App) -> Option<Moved> {
        let requests = self.0.upgrade()?;
        let login = requests.read(cx).login.clone().unwrap_or_default();
        let me = (me, login.as_str());
        let moved = requests.update(cx, |rq, _| rq.show_waiting(number, me));
        if let Some(notices) = self.notices() {
            notices.update(cx, |n, cx| n.show(number, &requests.read(cx).waiting, me));
        }
        moved
    }

    fn settle_waiting(&self, cx: &mut App) {
        let notices = self.notices();
        self.update(cx, |rq, cx| rq.settle_waiting(notices.as_ref(), cx));
    }

    fn is_temp(&self, id: u64) -> bool {
        crate::waiting::is_temp(id)
    }

    fn is_temp_thread(&self, id: &str) -> bool {
        crate::waiting::is_temp_thread(id)
    }

    fn read_notices(&self, me: &str, cx: &mut App) {
        if let Some(notices) = self.notices() {
            let me = me.to_owned();
            self.update(cx, |rq, cx| rq.read_notices(notices, me, cx));
        }
    }

    fn refresh_notices(&self, me: &str, cx: &mut App) {
        if let Some(notices) = self.notices() {
            notices.update(cx, |n, _| n.forget());
        }
        self.read_notices(me, cx);
    }

    fn open_notice(&self, ix: usize, cx: &mut App) -> Option<(u64, Option<u64>)> {
        let n = self.notices()?.update(cx, |n, _| n.open(ix))?;
        Some((n.iid, n.note))
    }

    fn dismiss_notice(&self, ix: usize, cx: &mut App) {
        if let Some(notices) = self.notices() {
            self.update(cx, |rq, cx| rq.dismiss_notice(notices, ix, cx));
        }
    }

    fn read_all_notices(&self, others: Vec<String>, cx: &mut App) {
        if let Some(notices) = self.notices() {
            notices.update(cx, |n, _| n.read_all(others));
        }
    }

    fn mark_read(&self, key: String, cx: &mut App) {
        if let Some(notices) = self.notices() {
            notices.update(cx, |n, _| n.mark_read(key));
        }
    }

    fn notice_by_key(&self, key: &str, cx: &App) -> Option<usize> {
        self.read_notices_with(cx, |n| n.list.iter().position(|n| n.key == key))?
    }

    fn notice_request(&self, ix: usize, cx: &App) -> Option<u64> {
        self.read_notices_with(cx, |n| n.list.get(ix).map(|n| n.iid))?
    }

    fn selected_notice(&self, cx: &App) -> Option<u64> {
        self.read_notices_with(cx, |n| n.list.get(n.at).map(|n| n.iid))?
    }

    fn unread_of(&self, number: u64, cx: &App) -> bool {
        self.read_notices_with(cx, |n| n.unread_of(number))
            .unwrap_or(false)
    }

    fn unread_with(&self, answers: artoo_requests_api::Answers, cx: &App) -> usize {
        self.read_notices_with(cx, |n| n.unread_with(answers))
            .unwrap_or(0)
    }

    fn waiting_threads(&self, number: u64, cx: &App) -> Vec<Thread> {
        let Some(requests) = self.0.upgrade() else {
            return Vec::new();
        };
        let rq = requests.read(cx);
        self.read_notices_with(cx, |n| {
            rq.waiting_for_me(n.threads_of(number).unwrap_or_default())
                .into_iter()
                .cloned()
                .collect()
        })
        .unwrap_or_default()
    }

    fn inbox_rows(&self, answers: artoo_requests_api::Answers, cx: &App) -> Vec<InboxRow> {
        let (Some(view), Some(notices)) = (self.inbox(), self.notices()) else {
            return Vec::new();
        };
        view.read(cx).rows(notices.read(cx), answers)
    }

    fn inbox_current(&self, answers: artoo_requests_api::Answers, cx: &App) -> Option<InboxRow> {
        self.inbox()?
            .read(cx)
            .current(self.notices()?.read(cx), answers)
    }

    fn inbox_step(
        &self,
        answers: artoo_requests_api::Answers,
        down: bool,
        cx: &App,
    ) -> Option<InboxRow> {
        self.inbox()?
            .read(cx)
            .step(self.notices()?.read(cx), answers, down)
    }

    fn inbox_pick(&self, row: Option<InboxRow>, cx: &mut App) {
        let (Some(view), Some(notices)) = (self.inbox(), self.notices()) else {
            return;
        };
        view.update(cx, |v, cx| {
            notices.update(cx, |n, _| crate::inbox::pick(row, v, n))
        });
    }

    fn read_excerpt(&self, cx: &mut App) {
        let (Some(view), Some(notices)) = (self.inbox(), self.notices()) else {
            return;
        };
        self.update(cx, |rq, cx| rq.read_excerpt(&notices, &view, cx));
    }

    fn set_inbox_filter(&self, filter: InboxFilter, cx: &mut App) {
        if let Some(view) = self.inbox() {
            view.update(cx, |v, _| v.filter = filter);
        }
    }

    fn inbox_filters(&self, on: OnInbox, cx: &App) -> AnyElement {
        let filter = self.inbox().map(|v| v.read(cx).filter).unwrap_or_default();
        crate::inbox_view::filters(filter, &on).into_any_element()
    }

    fn inbox_reply_to(&self, cx: &App) -> Option<artoo_requests_api::ReplyTo> {
        self.read_notices_with(cx, crate::inbox::reply_to)?
    }

    fn inbox_reply(&self, cx: &App) -> Option<(u64, Option<String>)> {
        self.inbox()?.read(cx).reply.clone()
    }

    fn set_inbox_reply(&self, reply: Option<(u64, Option<String>)>, cx: &mut App) {
        if let Some(view) = self.inbox() {
            view.update(cx, |v, _| v.reply = reply);
        }
    }

    fn follow(&self, number: Option<u64>, cx: &mut App) {
        let group = number
            .and_then(|n| self.find(n, cx))
            .map(|m| artoo_requests_api::Requests::group_of(self, &m, cx));
        self.update(cx, |rq, _| {
            if number != rq.current {
                rq.forget_open();
            }
            rq.current = number;
            if let Some(iid) = number {
                if rq.walk.is_none_or(|(walked, _)| walked != iid)
                    && let Some(group) = group
                {
                    rq.walk = Some((iid, group));
                }
                rq.mark_opened(iid);
            }
            rq.reading = false;
        });
    }

    fn reading(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|r| r.read(cx).reading)
    }

    fn fetch_head_once(&self, number: u64, cx: &mut App) -> bool {
        self.update(cx, |rq, _| rq.heads_fetched.insert(number))
            .unwrap_or(false)
    }

    fn fetch_sides_once(&self, number: u64, cx: &mut App) -> bool {
        self.update(cx, |rq, _| rq.sides_fetched.insert(number))
            .unwrap_or(false)
    }

    fn host_sides(&self, number: u64, cx: &App) -> Option<HostSides> {
        let (iid, sides) = self.0.upgrade()?.read(cx).host?;
        (iid == number).then_some(sides)
    }

    fn shown(&self, filter: &str, cx: &App) -> Vec<Mr> {
        let Some(requests) = self.0.upgrade() else {
            return Vec::new();
        };
        requests.read(cx).shown(filter, &|m| self.seen(m, cx))
    }

    fn next_request(&self, filter: &str, cx: &App) -> Option<Mr> {
        self.0
            .upgrade()?
            .read(cx)
            .next_request(filter, &|m| self.seen(m, cx))
    }

    fn group_of(&self, m: &Mr, cx: &App) -> Group {
        let seen = self.seen(m, cx);
        self.0
            .upgrade()
            .map_or(Group::New, |r| r.read(cx).group_of(m, seen))
    }

    fn view(&self, cx: &App) -> mr::MrState {
        self.0
            .upgrade()
            .map(|r| r.read(cx).view)
            .unwrap_or_default()
    }

    fn set_scope(&self, scope: Scope, cx: &mut App) {
        self.update(cx, |rq, cx| {
            rq.scope = scope;
            cx.notify();
        });
    }

    fn scope_count(&self, scope: Scope, cx: &App) -> String {
        let Some(requests) = self.0.upgrade() else {
            return String::new();
        };
        let rq = requests.read(cx);
        // The merged or the closed: as many as the host says, or counted as far as read (#114, #169).
        if rq.view == mr::MrState::Opened {
            let n = rq.list.iter().filter(|m| rq.in_scope(m, scope)).count();
            return n.to_string();
        }
        let pages = rq.past.of_scope(scope);
        match pages.total {
            Some(total) => total.to_string(),
            None => format!("{}{}", pages.list.len(), if pages.more { "+" } else { "" }),
        }
    }

    fn toggle_group(&self, group: Group, cx: &mut App) -> String {
        use crate::group::Band;
        self.update(cx, |rq, cx| {
            rq.rows.toggle(group);
            cx.notify();
        });
        format!("group-{}", group.key())
    }

    fn pick_row(&self, number: Option<u64>, cx: &mut App) {
        self.update(cx, |rq, _| rq.rows.picked = number);
    }

    fn reveal_row(&self, cx: &mut App) {
        if let Some(requests) = self.0.upgrade() {
            requests.read(cx).rows.reveal.set(true);
        }
    }

    fn folds(&self, group: Group, filtering: bool, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|r| r.read(cx).rows.folds(group, filtering))
    }

    fn row_opened(&self, number: u64, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|r| r.read(cx).rows.opened(number))
    }

    fn load_rows(&self, key: &str, cx: &mut App) {
        self.update(cx, |rq, _| rq.rows.load(key));
    }

    fn render_thread(&self, ix: usize, look: ThreadLook, on: OnThread, cx: &App) -> AnyElement {
        let Some(requests) = self.0.upgrade() else {
            return gpui::div().into_any_element();
        };
        crate::thread::render_thread(&requests, ix, look, self.acts(), on, cx)
    }

    fn render_draft(&self, ix: usize, look: DraftLook, on: OnDraft, cx: &App) -> AnyElement {
        let Some(requests) = self.0.upgrade() else {
            return gpui::div().into_any_element();
        };
        crate::thread::render_draft(requests.read(cx), ix, look, on, cx)
    }

    fn drafts_in(&self, path: &str, cx: &App) -> Vec<(usize, bool, u32)> {
        self.0
            .upgrade()
            .map(|r| r.read(cx).drafts_in(path))
            .unwrap_or_default()
    }

    fn draft_thread(&self, ix: usize, cx: &App) -> Option<usize> {
        let requests = self.0.upgrade()?;
        let rq = requests.read(cx);
        rq.draft_thread(rq.drafts.get(ix)?)
    }

    fn open_submit(&self, cx: &mut App) -> bool {
        self.update(cx, |rq, _| rq.open_submit_card())
            .unwrap_or(false)
    }

    fn close_submit(&self, cx: &mut App) {
        self.update(cx, |rq, cx| rq.close_submit(cx));
    }

    fn submitting(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|r| r.read(cx).submitting.is_some())
    }

    fn submit_key(&self, event: &KeyDownEvent, cx: &mut App) -> bool {
        self.update(cx, |rq, cx| rq.submit_key(event, cx))
            .unwrap_or(false)
    }

    fn submit_card(
        &self,
        look: SubmitLook,
        cx: &mut App,
    ) -> Option<(gpui::Stateful<gpui::Div>, gpui::Div)> {
        let acts = self.acts();
        self.update(cx, |rq, cx| rq.submit_card(look, acts, cx))?
    }

    fn open_create(
        &self,
        branch: &BranchRef,
        drafted: Option<Drafted>,
        cx: &mut App,
    ) -> Option<Pair> {
        let requests = self.0.upgrade()?;
        let host = requests.read(cx).host(cx)?;
        let branches = self.branch_list(cx);
        let here = requests.read(cx).here()?;
        requests.update(cx, |rq, cx| {
            rq.open_create(&here.of(&branches), branch, drafted, host, cx)
        })
    }

    fn use_template(&self, ix: usize, cx: &mut App) {
        self.update(cx, |rq, _| rq.use_template(ix));
    }

    fn open_create_menu(&self, name: &str, cx: &mut App) {
        self.update(cx, |rq, cx| rq.open_create_menu(name, cx));
    }

    fn create_quick(&self, cx: &mut App) {
        self.update(cx, |rq, _| rq.create_quick());
    }

    fn creating(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|r| r.read(cx).creating.is_some())
    }

    fn pick_branch(&self, name: String, source: bool, cx: &mut App) -> Option<Pair> {
        let branches = self.branch_list(cx);
        let here = self.0.upgrade()?.read(cx).here()?;
        self.update(cx, |rq, _| {
            let here = here.of(&branches);
            if source {
                rq.pick_source(&here, name)
            } else {
                rq.pick_target(&here, name)
            }
        })?
    }

    fn step_branch_list(&self, down: bool, cx: &mut App) -> bool {
        let branches = self.branch_list(cx);
        self.update(cx, |rq, cx| {
            let took = rq.step_branch_list(&branches, down);
            if took {
                cx.notify();
            }
            took
        })
        .unwrap_or(false)
    }

    fn render_create_quick(
        &self,
        typing: bool,
        selecting: Selecting,
        cx: &mut App,
    ) -> Option<AnyElement> {
        let requests = self.0.upgrade()?;
        let branches = self.branch_list(cx);
        let shown = self.review().and_then(|r| r.shown(cx));
        let web = requests.read(cx).web.clone();
        let look = crate::create::CreateLook {
            typing,
            github: web.as_ref().is_some_and(|w| w.github),
            base: web.map(|w| w.base),
            selecting: Some(selecting),
            totals: shown.as_ref().and_then(|d| crate::create::Totals::of(d)),
            branches: &branches,
        };
        let acts = self.acts();
        requests.update(cx, |rq, cx| rq.render_create_quick(&look, acts, cx))
    }

    fn start_since(&self, ix: usize, latest: usize, cx: &mut App) -> Option<usize> {
        self.update(cx, |rq, _| {
            let iid = rq.current?;
            let t = rq.threads.get(ix)?;
            let (h, path) = (rq.histories.get(&t.id)?, t.path.clone()?);
            let stops = h.stops(t.new_line.or(t.old_line), latest);
            let last = stops.len().checked_sub(1)?;
            let thread = t.id.clone();
            rq.since = Some(Since {
                iid,
                thread,
                path,
                stops,
            });
            Some(last)
        })?
    }

    fn since(&self, cx: &App) -> Option<Since> {
        self.0.upgrade()?.read(cx).since.clone()
    }

    fn leave_since(&self, cx: &mut App) -> Option<u64> {
        self.update(cx, |rq, _| rq.since.take().map(|s| s.iid))?
    }

    fn on_event(&self, hear: artoo_requests_api::Hears, cx: &mut App) {
        if let Some(requests) = self.0.upgrade() {
            cx.subscribe(&requests, move |_, event: &Event, cx| hear(event, cx))
                .detach();
        }
    }

    fn on_change(&self, hear: Rc<dyn Fn(&mut App)>, cx: &mut App) {
        if let Some(requests) = self.0.upgrade() {
            cx.observe(&requests, move |_, cx| hear(cx)).detach();
        }
    }

    fn line_changed(&self, thread: &str, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|r| {
            r.read(cx)
                .histories
                .get(thread)
                .is_some_and(crate::outdated::LineHistory::changed)
        })
    }
}
