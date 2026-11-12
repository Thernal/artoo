//! The branch open: the repository's branches, which one is open and what its page says of it, its versions and
//! its commits — a page of them, older ones read as the list asks —, and what of them the diff shows.

use crate::compare::CompareCommits;
use crate::facts::Facts;
use crate::find::Find;
use crate::opening::Shows;
use crate::reading::Read;
use crate::roots::Roots;
use crate::waiting::Waiting;
use crate::working::WorkingState;
use artoo_core::git::{BranchRef, CommitInfo, Deco, Version};
use gpui::{EventEmitter, Task, UniformListScrollHandle};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

pub use artoo_branches_api::{NamedTip, RequestSides};

pub use artoo_branches_api::{Act, Selection, VersionTab};

/// More of a branch's history to read past the commits listed, from `tip` (#25).
pub struct Older {
    pub tip: git2::Oid,
    pub task: Option<Task<()>>,
}

pub struct History {
    /// The repository whose history this is: what every read opens.
    pub root: Option<PathBuf>,
    /// Shared, so what reads them keeps them while the tab changes something else.
    pub branches: Arc<Vec<BranchRef>>,
    pub branch: Option<usize>,
    /// The loaded branch's page: its main line, how far from it, where it is pushed (#74).
    pub page: Option<Facts>,
    /// A comparison's own commits, and the run of them its diff is narrowed to (#21).
    pub compare: Option<CompareCommits>,
    pub versions: Vec<Version>,
    pub commits: Vec<CommitInfo>,
    /// None: `commits` is all the history there is to list.
    pub older: Option<Older>,
    pub selection: Selection,
    pub version_tab: VersionTab,
    /// A merge request's diff to show once its branch is read (#71).
    pub pending_request: Option<RequestSides>,
    /// The merge request whose diff is open (or was last opened from the list).
    pub open_request: Option<RequestSides>,
    /// A page waiting for what it needs: the branch's commits or versions, a request's (`waiting`).
    pub waiting: Option<Waiting>,
    /// What the route last shown came to (`show_opened`), for the tab to take as soon as it is shown.
    pub shown: Option<Shows>,
    /// The commit's message open under its title, over its diff (`head`).
    pub message_open: bool,
    /// Where the Commits section's list is scrolled.
    pub commit_scroll: UniformListScrollHandle,
    /// Where a comparison's commits are scrolled (#21).
    pub compare_scroll: UniformListScrollHandle,
    /// Labels on commits: where branches, remote branches and tags point.
    pub decor: HashMap<git2::Oid, Vec<Deco>>,
    /// The commits' search (⌘⇧F).
    pub find: Find,
    /// What the Branches list reads for its rows and previews (#72).
    pub roots: Roots,
    /// The working tree as last looked at: the tab's poll keeps it.
    pub wt: WorkingState,
    /// The branch being read; replacing it drops, and so cancels, the one it replaces.
    pub task: Option<Task<()>>,
    /// The branch read, until the tab shows it (`History::settle`).
    pub(crate) read: Option<Read>,
    /// How its sections of the side panel are folded.
    pub sections: crate::panel::Sections,
    /// Newer commits the watch found on the branch, until the user asks for them (`news`).
    pub pending: Option<crate::news::Pending>,
    /// Versions from this index on arrived since the user last looked.
    pub new_from: Option<usize>,
    /// How much of each commit you viewed, as the review last said it (#209): its row says `1 of 3` or `viewed`.
    pub viewed: std::collections::HashMap<git2::Oid, (usize, usize)>,
    /// The repository read again after it changed (`reload`).
    pub reload: Option<Task<()>>,
    /// The commit the Commits page has picked (#73), and with shift the other end of a run of them
    /// (`commits_page`).
    pub pick: Option<git2::Oid>,
    pub pick_to: Option<git2::Oid>,
    /// Its lists' widths: the Commits section's (`side-commits`), the Commits page's (`commits`).
    pub widths: artoo_ui_kit::room::Widths,
    /// The version you read of the request open, kept while it stays open (#187).
    pub kept_read: Option<crate::since::KeptRead>,
    /// How many files the request open's whole diff lists, read once for its sides (base, head) (#187).
    pub whole: Option<((git2::Oid, git2::Oid), usize)>,
    /// The main line's page shows its Changes tab, not its Commits (#208).
    pub main_changes: bool,
}

pub use artoo_branches_api::Event;

impl EventEmitter<Event> for History {}

pub use artoo_branches_api::Acts;

impl History {
    /// A tab's history: its sections of the panel folded as the user left them.
    pub fn new() -> Self {
        Self {
            sections: crate::panel::Sections::load(),
            ..Default::default()
        }
    }

    /// The versions a click on version `ix` compares: with ⌘ (`more`) from the one picked to it, on the latest
    /// the one before it to it, else it to the latest.
    pub fn versions_clicked(&self, ix: usize, more: bool) -> (usize, usize) {
        let latest = self.versions.len() - 1;
        match (self.selection, more) {
            (Selection::Versions { from, .. }, true) if from != ix => (from.min(ix), from.max(ix)),
            _ if ix == latest => (latest - 1, latest),
            _ => (ix, latest),
        }
    }

    /// ↑ ↓ in the commit list: the row the selection moves to, if it moves. With changes in the working tree
    /// (`working`) its row is the one above the first commit.
    pub fn step_row(&self, down: bool, working: bool) -> Option<Row> {
        let working = working || self.selection == Selection::WorkingTree;
        let first = (self.commit_rows() > 0).then(|| Row::Commit(self.commit_at(0)));
        match (self.selection, down) {
            (Selection::WorkingTree, true) => first,
            (Selection::WorkingTree, false) => None,
            (Selection::Commit(ix), false) if working && self.row_of(ix) == Some(0) => {
                Some(Row::WorkingTree)
            }
            (Selection::Commit(ix), _) => {
                let row = self.row_of(ix)?;
                let to = if down {
                    (row + 1 < self.commit_rows()).then(|| row + 1)
                } else {
                    row.checked_sub(1)
                };
                to.map(|to| Row::Commit(self.commit_at(to)))
            }
            (_, true) => first,
            _ => None,
        }
    }

    /// Whether the host has commit `sha`: a remote branch or a request it fetched has it; taken as so while the
    /// repository cannot be read.
    pub fn pushed(&self, sha: git2::Oid) -> bool {
        let Some(repo) = (self.root.as_ref()).and_then(|r| artoo_core::git::Repo::open(r).ok())
        else {
            return true;
        };
        (self.branches.iter())
            .filter(|b| b.kind != artoo_core::git::RefKind::Local)
            .any(|b| repo.contains(b.tip, sha))
    }

    /// The first parent of commit `sha`; none for one that starts the history.
    pub fn parent(&self, sha: git2::Oid) -> Option<git2::Oid> {
        let repo = artoo_core::git::Repo::open(self.root.as_ref()?).ok()?;
        repo.parent_of(sha).ok()
    }
}

/// A row of the commit list: the working tree's, or commit `ix` of `History::commits`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    WorkingTree,
    Commit(usize),
}

impl Default for History {
    fn default() -> Self {
        History {
            root: None,
            branches: Arc::default(),
            branch: None,
            page: None,
            compare: None,
            versions: Vec::new(),
            commits: Vec::new(),
            older: None,
            selection: Selection::None,
            version_tab: VersionTab::Files,
            pending_request: None,
            open_request: None,
            waiting: None,
            shown: None,
            message_open: false,
            commit_scroll: UniformListScrollHandle::new(),
            compare_scroll: UniformListScrollHandle::new(),
            decor: HashMap::new(),
            find: Find::default(),
            roots: Roots::default(),
            wt: WorkingState::default(),
            task: None,
            read: None,
            sections: Default::default(),
            pending: None,
            new_from: None,
            viewed: Default::default(),
            reload: None,
            pick: None,
            pick_to: None,
            widths: Default::default(),
            kept_read: None,
            main_changes: false,
            whole: None,
        }
    }
}

/// The history's branches as a part reads them (`artoo_branches_api::Branches`); a click on its commits asks the
/// tab's acts in `services`.
pub fn handle(
    history: &gpui::Entity<History>,
    services: &std::rc::Rc<artoo_shell_api::Services>,
) -> std::rc::Rc<dyn artoo_branches_api::Branches> {
    std::rc::Rc::new(Handle(
        history.downgrade(),
        std::rc::Rc::downgrade(services),
    ))
}

/// Weak: the services hold it.
struct Handle(
    gpui::WeakEntity<History>,
    std::rc::Weak<artoo_shell_api::Services>,
);

impl Handle {
    /// The tab's review, which the history's diffs open in.
    fn review(&self) -> Option<std::rc::Rc<dyn artoo_review_api::Review>> {
        self.1.upgrade()?.get()
    }
}

impl artoo_branches_api::Branches for Handle {
    fn tabbed(&self, refname: &str, cx: &gpui::App) -> bool {
        self.0.upgrade().is_some_and(|h| h.read(cx).tabbed(refname))
    }

    fn main_line(&self, refname: &str, cx: &gpui::App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|h| h.read(cx).main_line(refname))
    }

    fn main_changes(&self, cx: &gpui::App) -> bool {
        self.0.upgrade().is_some_and(|h| h.read(cx).main_changes)
    }

    fn set_main_changes(&self, on: bool, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, cx| {
                h.main_changes = on;
                cx.notify();
            });
        }
    }

    fn pick_commits(&self, newest: git2::Oid, oldest: Option<git2::Oid>, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.pick_commits(newest, oldest));
        }
    }

    fn settling(&self, cx: &gpui::App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|h| h.read(cx).pending_request.is_some())
    }

    fn back_to_list(&self, route: &artoo_core::route::Route, cx: &mut gpui::App) {
        use artoo_branches_api::{BranchRoute, WorkingTreeRoute};
        use artoo_core::route::Place;
        let Some(h) = self.0.upgrade() else {
            return;
        };
        let row = if WorkingTreeRoute::from_route(route).is_some() {
            Some(crate::roots::BranchRow::WorkingTree)
        } else if let Some(branch) = BranchRoute::from_route(route) {
            let ix = h
                .read(cx)
                .branches
                .iter()
                .position(|b| b.refname == branch.refname);
            ix.map(crate::roots::BranchRow::Branch)
        } else {
            return;
        };
        h.update(cx, |h, _| h.roots.picked = row);
    }

    fn route_of(&self, selection: Selection, cx: &gpui::App) -> Option<artoo_core::route::Route> {
        let current = self
            .1
            .upgrade()?
            .get::<std::rc::Rc<dyn artoo_requests_api::Requests>>()
            .and_then(|r| r.current(cx));
        self.0.upgrade()?.read(cx).route_of(selection, current)
    }

    fn take_shown(&self, cx: &mut gpui::App) -> Option<artoo_branches_api::Shows> {
        self.0.upgrade()?.update(cx, |h, _| h.shown.take())
    }

    fn finish_waiting(&self, cx: &mut gpui::App) -> Option<artoo_branches_api::Finished> {
        let review = self
            .1
            .upgrade()?
            .get::<std::rc::Rc<dyn artoo_review_api::Review>>()?;
        self.0
            .upgrade()?
            .update(cx, |h, cx| h.finish_waiting(&*review, cx))
    }

    fn opened(
        &self,
        root: PathBuf,
        branches: Vec<BranchRef>,
        decor: Option<HashMap<git2::Oid, Vec<Deco>>>,
        fresh: bool,
        cx: &mut gpui::App,
    ) {
        let Some(h) = self.0.upgrade() else {
            return;
        };
        h.update(cx, |h, _| {
            if fresh {
                h.find.reset();
            }
            if let Some(decor) = decor {
                h.decor = decor;
                h.forget_news();
            }
            h.root = Some(root);
            h.branches = Arc::new(branches);
        });
    }

    fn first_branch(&self, refname: Option<&str>, cx: &gpui::App) -> Option<usize> {
        let h = self.0.upgrade()?;
        let branches = &h.read(cx).branches;
        refname
            .and_then(|r| branches.iter().position(|b| b.refname == r))
            .or_else(|| branches.iter().position(|b| b.is_head))
            .or((!branches.is_empty()).then_some(0))
    }

    fn open_branch(&self, ix: usize, keep: Option<git2::Oid>, cx: &mut gpui::App) -> bool {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return false;
        };
        h.update(cx, |h, cx| h.open_branch(ix, keep, &*review, cx))
    }

    fn forget(&self, cx: &mut gpui::App) {
        if let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) {
            h.update(cx, |h, cx| h.forget(&*review, cx));
        }
    }

    fn clear_diff(&self, cx: &mut gpui::App) {
        if let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) {
            h.update(cx, |h, cx| h.clear_diff(&*review, cx));
        }
    }

    fn show_commit(
        &self,
        ix: usize,
        keep: Option<String>,
        cx: &mut gpui::App,
    ) -> Option<CommitInfo> {
        let review = self.review()?;
        self.0
            .upgrade()?
            .update(cx, |h, cx| h.show_commit(ix, keep, &*review, cx))
    }

    fn show_commits(
        &self,
        newest: usize,
        oldest: usize,
        keep: Option<String>,
        cx: &mut gpui::App,
    ) -> bool {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return false;
        };
        h.update(cx, |h, cx| {
            h.show_commits(newest, oldest, keep, &*review, cx)
        })
    }

    fn show_versions(
        &self,
        from: usize,
        to: usize,
        keep: Option<String>,
        cx: &mut gpui::App,
    ) -> bool {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return false;
        };
        h.update(cx, |h, cx| h.show_versions(from, to, keep, &*review, cx))
    }

    fn show_interdiff(&self, pair: usize, cx: &mut gpui::App) -> bool {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return false;
        };
        h.update(cx, |h, cx| h.show_interdiff(pair, &*review, cx))
    }

    fn show_read(&self, cx: &mut gpui::App) -> Option<artoo_branches_api::Settled> {
        let review = self.review()?;
        let versions = self
            .1
            .upgrade()?
            .get::<std::rc::Rc<dyn artoo_requests_api::Requests>>()
            .and_then(|r| r.versions(cx));
        self.0
            .upgrade()?
            .update(cx, |h, cx| h.show_read(versions.as_ref(), &*review, cx))
    }

    fn branch(&self, cx: &gpui::App) -> Option<usize> {
        self.0.upgrade()?.read(cx).branch
    }

    fn selection(&self, cx: &gpui::App) -> Selection {
        self.0
            .upgrade()
            .map_or(Selection::None, |h| h.read(cx).selection)
    }

    fn commit(&self, ix: usize, cx: &gpui::App) -> Option<CommitInfo> {
        self.0.upgrade()?.read(cx).commits.get(ix).cloned()
    }

    fn commit_ix(&self, prefix: &str, cx: &gpui::App) -> Option<usize> {
        self.0
            .upgrade()?
            .read(cx)
            .commits
            .iter()
            .position(|c| c.id.to_string().starts_with(prefix))
    }

    fn search(&self, text: String, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| {
                h.find.text = Some(text);
                h.refilter();
            });
        }
    }

    fn on_event(&self, hear: artoo_branches_api::Hears, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            cx.subscribe(&h, move |_, event: &Event, cx| hear(event, cx))
                .detach();
        }
    }

    fn reload(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, cx| h.reload(cx));
        }
    }

    fn forget_news(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.forget_news());
        }
    }

    fn apply_pending(&self, cx: &mut gpui::App) -> Option<(artoo_branches_api::Applied, usize)> {
        self.0.upgrade()?.update(cx, |h, cx| h.apply_pending(cx))
    }

    fn listed(&self, cx: &gpui::App) -> Vec<(usize, BranchRef)> {
        self.0.upgrade().map_or_else(Vec::new, |h| {
            h.read(cx).listed().map(|(ix, b)| (ix, b.clone())).collect()
        })
    }

    fn rows(&self, filter: &str, cx: &gpui::App) -> Vec<crate::roots::BranchRow> {
        self.0
            .upgrade()
            .map_or_else(Vec::new, |h| h.read(cx).rows(filter))
    }

    fn picked_row(&self, cx: &gpui::App) -> Option<crate::roots::BranchRow> {
        let h = self.0.upgrade()?;
        let h = h.read(cx);
        h.roots.picked.or_else(|| h.first_row())
    }

    fn pick_row(&self, row: Option<crate::roots::BranchRow>, cx: &mut gpui::App) {
        let Some(h) = self.0.upgrade() else {
            return;
        };
        h.update(cx, |h, _| h.roots.picked = row);
        if row == Some(crate::roots::BranchRow::WorkingTree) {
            self.read_tree(cx);
        }
    }

    fn count_line(&self, cx: &gpui::App) -> String {
        self.0
            .upgrade()
            .map(|h| h.read(cx).count_line())
            .unwrap_or_default()
    }

    fn toggle_remote(&self, remote: String, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.toggle_remote(remote));
        }
    }

    fn read_facts(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, cx| h.read_facts(cx));
        }
    }

    fn read_tree(&self, cx: &mut gpui::App) {
        let settings = self.review().map(|r| r.settings(cx)).unwrap_or_default();
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, cx| h.read_tree(settings, cx));
        }
    }

    fn open_find(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.open_find());
        }
    }

    fn close_find(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.close_find());
        }
    }

    fn searching(&self, cx: &gpui::App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|h| h.read(cx).find.text.is_some())
    }

    fn refilter(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.refilter());
        }
    }

    fn index_paths(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, cx| h.index_paths(cx));
        }
    }

    fn start_working_poll(
        &self,
        plain: bool,
        shown: std::rc::Rc<std::cell::Cell<bool>>,
        cx: &mut gpui::App,
    ) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, cx| h.start_working_poll(plain, shown, cx));
        }
    }

    fn show_working_tree(&self, keep: Option<String>, cx: &mut gpui::App) {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return;
        };
        h.update(cx, |h, cx| {
            let only_unstaged = h.wt.only_unstaged;
            h.show_working_tree(only_unstaged, keep, &*review, cx)
        });
    }

    fn set_only_unstaged(&self, on: bool, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.wt.only_unstaged = on);
        }
    }

    fn stop_waiting(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.waiting = None);
        }
    }

    fn loaded(
        &self,
        on_screen: Option<&artoo_core::route::Route>,
        cx: &gpui::App,
    ) -> Option<artoo_core::route::Route> {
        let services = self.1.upgrade()?;
        let shown = services
            .get::<std::rc::Rc<dyn artoo_review_api::Review>>()
            .and_then(|r| r.shown(cx));
        let current = services
            .get::<std::rc::Rc<dyn artoo_requests_api::Requests>>()
            .and_then(|r| r.current(cx));
        self.0.upgrade()?.read(cx).loaded_route(
            shown.as_ref().map(|d| &d.header),
            current,
            on_screen,
        )
    }

    fn list(&self, cx: &gpui::App) -> Arc<Vec<BranchRef>> {
        self.0
            .upgrade()
            .map(|h| h.read(cx).branches.clone())
            .unwrap_or_default()
    }

    fn commit_count(&self, cx: &gpui::App) -> usize {
        self.0.upgrade().map_or(0, |h| h.read(cx).commits.len())
    }

    fn versions(&self, cx: &gpui::App) -> Vec<(usize, git2::Oid)> {
        self.0.upgrade().map_or_else(Vec::new, |h| {
            h.read(cx)
                .versions
                .iter()
                .map(|v| (v.number, v.tip))
                .collect()
        })
    }

    fn panel(&self, width: f32, cx: &mut gpui::App) -> Option<gpui::AnyElement> {
        use gpui::IntoElement;
        let panel = self
            .1
            .upgrade()?
            .get::<gpui::Entity<crate::panel::Panel>>()?;
        panel.update(cx, |p, _| p.width = width);
        Some(panel.into_any_element())
    }

    fn commits_page(&self, versions: bool, cx: &mut gpui::App) -> gpui::AnyElement {
        use gpui::IntoElement;
        let Some(h) = self.0.upgrade() else {
            return gpui::div().into_any_element();
        };
        let acts = self
            .1
            .upgrade()
            .and_then(|s| s.get::<Acts>())
            .unwrap_or_else(|| std::rc::Rc::new(|_, _| {}));
        h.update(cx, |h, cx| h.commits_page(versions, &acts, cx))
    }

    fn show(&self, route: &artoo_core::route::Route, cx: &mut gpui::App) {
        let (Some(h), Some(services)) = (self.0.upgrade(), self.1.upgrade()) else {
            return;
        };
        let Some(review) = services.get::<std::rc::Rc<dyn artoo_review_api::Review>>() else {
            return;
        };
        let current = services
            .get::<std::rc::Rc<dyn artoo_requests_api::Requests>>()
            .and_then(|r| r.current(cx));
        h.update(cx, |h, cx| h.show_opened(route, current, &*review, cx));
    }

    fn commit_of(&self, id: git2::Oid, cx: &gpui::App) -> Option<usize> {
        self.0
            .upgrade()?
            .read(cx)
            .commits
            .iter()
            .position(|c| c.id == id)
    }

    fn commits_where(
        &self,
        keep: &dyn Fn(&CommitInfo) -> bool,
        limit: usize,
        cx: &gpui::App,
    ) -> Vec<CommitInfo> {
        self.0.upgrade().map_or_else(Vec::new, |h| {
            h.read(cx)
                .commits
                .iter()
                .filter(|c| keep(c))
                .take(limit)
                .cloned()
                .collect()
        })
    }

    fn version(&self, ix: usize, cx: &gpui::App) -> Option<Version> {
        self.0.upgrade()?.read(cx).versions.get(ix).cloned()
    }

    fn latest_version(&self, cx: &gpui::App) -> Option<Version> {
        self.0.upgrade()?.read(cx).versions.last().cloned()
    }

    fn pushed(&self, sha: git2::Oid, cx: &gpui::App) -> bool {
        self.0.upgrade().is_some_and(|h| h.read(cx).pushed(sha))
    }

    fn parent(&self, sha: git2::Oid, cx: &gpui::App) -> Option<git2::Oid> {
        self.0.upgrade()?.read(cx).parent(sha)
    }

    fn show_version_files(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.version_tab = VersionTab::Files);
        }
    }

    fn range(
        &self,
        route: Option<&artoo_core::route::Route>,
        cx: &gpui::App,
    ) -> Option<artoo_branches_api::RangeOn> {
        let review = self.review()?;
        self.0.upgrade()?.read(cx).range_on(route?, &*review, cx)
    }

    fn range_pick(&self, range: artoo_branches_api::Range, cx: &gpui::App) -> Option<Selection> {
        let review = self.review()?;
        self.0.upgrade()?.read(cx).range_pick(range, &*review, cx)
    }

    fn unchanged(&self, files: usize, cx: &gpui::App) -> Option<usize> {
        let review = self.review()?;
        self.0.upgrade()?.read(cx).unchanged(files, &*review, cx)
    }

    fn keep_read(&self, cx: &mut gpui::App) {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return;
        };
        h.update(cx, |h, cx| h.keep_read(&*review, cx));
    }

    fn commits_folded(&self, cx: &gpui::App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|h| h.read(cx).sections.commits)
    }

    fn message_open(&self, cx: &gpui::App) -> bool {
        self.0.upgrade().is_some_and(|h| h.read(cx).message_open)
    }

    fn has_news(&self, cx: &gpui::App) -> bool {
        self.0.upgrade().is_some_and(|h| h.read(cx).has_news())
    }

    fn reading(&self, cx: &gpui::App) -> bool {
        self.0.upgrade().is_some_and(|h| h.read(cx).task.is_some())
    }

    fn drop_find(&self, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.find.text = None);
        }
    }

    fn step_row(&self, down: bool, cx: &gpui::App) -> Option<Selection> {
        let h = self.0.upgrade()?;
        let h = h.read(cx);
        Some(match h.step_row(down, h.wt.count > 0)? {
            Row::Commit(ix) => Selection::Commit(ix),
            Row::WorkingTree => Selection::WorkingTree,
        })
    }

    fn step_pick(&self, down: bool, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, cx| {
                h.step_pick(down);
                cx.notify()
            });
        }
    }

    fn picked(&self, cx: &gpui::App) -> Option<Selection> {
        let h = self.0.upgrade()?;
        let h = h.read(cx);
        match h.picked_run() {
            Some((newest, oldest)) => Some(Selection::Commits { newest, oldest }),
            None => h.picked().map(Selection::Commit),
        }
    }

    fn panel_shows(
        &self,
        route: Option<&artoo_core::route::Route>,
        cx: &gpui::App,
    ) -> artoo_branches_api::PanelShows {
        self.0
            .upgrade()
            .map(|h| h.read(cx).panel_shows(route))
            .unwrap_or_default()
    }

    fn count_files(&self, cx: &mut gpui::App) {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return;
        };
        let Some(d) = review.shown(cx) else {
            return;
        };
        h.update(cx, |h, _| {
            if h.shows_branch(Some(&d.header))
                && let Some(f) = h.page.as_mut()
            {
                f.files = Some(d.files.len());
            }
        });
    }

    fn branch_files(&self, cx: &gpui::App) -> Option<usize> {
        self.0.upgrade()?.read(cx).page.as_ref()?.files
    }

    fn commit_card(
        &self,
        id: git2::Oid,
        web: Option<&artoo_core::git::WebRemote>,
        cx: &gpui::App,
    ) -> Option<artoo_branches_api::Card> {
        let h = self.0.upgrade()?;
        let h = h.read(cx);
        let info = h.commits.iter().find(|c| c.id == id)?.clone();
        let card = h.card(info, web);
        Some(std::rc::Rc::new(move |_, cx| {
            crate::card::CommitCard::show(&card, cx)
        }))
    }

    fn branch_card(
        &self,
        ix: usize,
        request: Option<(u64, String)>,
        cx: &gpui::App,
    ) -> Option<artoo_branches_api::Card> {
        let build = crate::card::branch_card(self.0.upgrade()?.read(cx), ix, request)?;
        Some(std::rc::Rc::new(build))
    }

    fn compare(
        &self,
        base: NamedTip,
        head: NamedTip,
        since: bool,
        request: Option<u64>,
        cx: &mut gpui::App,
    ) {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return;
        };
        h.update(cx, |h, cx| {
            h.show_compare(base, head, since, request, &*review, cx)
        });
    }

    fn compare_listed(&self, route: Option<&artoo_core::route::Route>, cx: &gpui::App) -> bool {
        let (Some(h), Some(route)) = (self.0.upgrade(), route) else {
            return false;
        };
        h.read(cx).compare_of(route).is_some()
    }

    fn compare_narrowed(&self, route: Option<&artoo_core::route::Route>, cx: &gpui::App) -> bool {
        let (Some(h), Some(route)) = (self.0.upgrade(), route) else {
            return false;
        };
        let h = h.read(cx);
        h.compare_of(route).is_some_and(|c| c.picked.is_some()) && h.selection == Selection::None
    }

    fn read_compare(&self, route: Option<&artoo_core::route::Route>, cx: &mut gpui::App) {
        let Some(h) = self.0.upgrade() else {
            return;
        };
        use artoo_core::route::Place;
        let of = route
            .and_then(artoo_branches_api::CompareRoute::from_route)
            .and_then(|r| {
                let oid = |hex: &str| git2::Oid::from_str(hex).ok();
                let base = (r.base.clone(), oid(&r.base_commit)?);
                Some((base, (r.head.clone(), oid(&r.head_commit)?), r.since))
            });
        h.update(cx, |h, cx| match of {
            Some(of) => h.read_compare(of, cx),
            None => h.compare = None,
        });
    }

    fn pick_compare(
        &self,
        ix: usize,
        span: bool,
        keep: Option<String>,
        cx: &mut gpui::App,
    ) -> bool {
        let Some(h) = self.0.upgrade() else {
            return false;
        };
        let picked = h.update(cx, |h, _| h.compare.as_mut().map(|c| c.pick(ix, span)));
        picked.is_some() && self.show_compare_pick(keep, cx)
    }

    fn show_compare_pick(&self, keep: Option<String>, cx: &mut gpui::App) -> bool {
        let (Some(h), Some(review)) = (self.0.upgrade(), self.review()) else {
            return false;
        };
        h.update(cx, |h, cx| h.show_compare_pick(keep, &*review, cx))
    }

    fn step_compare(
        &self,
        route: Option<&artoo_core::route::Route>,
        down: bool,
        keep: Option<String>,
        cx: &mut gpui::App,
    ) -> bool {
        let (Some(h), Some(route)) = (self.0.upgrade(), route) else {
            return false;
        };
        let Some(to) = h.read(cx).compare_of(route).and_then(|c| c.step(down)) else {
            return false;
        };
        let shown = self.pick_compare(to, false, keep, cx);
        h.read(cx)
            .compare_scroll
            .scroll_to_item(to, gpui::ScrollStrategy::Nearest);
        shown
    }

    fn compare_title(&self, route: &artoo_core::route::Route, cx: &gpui::App) -> Option<String> {
        self.0.upgrade()?.read(cx).compare_title(route)
    }

    fn request_sides(&self, cx: &gpui::App) -> (Option<RequestSides>, Option<RequestSides>) {
        self.0.upgrade().map_or((None, None), |h| {
            let h = h.read(cx);
            (h.pending_request.clone(), h.open_request.clone())
        })
    }

    fn set_pending_request(&self, sides: Option<RequestSides>, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.pending_request = sides);
        }
    }

    fn set_open_request(&self, sides: Option<RequestSides>, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.open_request = sides);
        }
    }

    fn request_branch(
        &self,
        of: &crate::opening::RequestOf,
        cx: &gpui::App,
    ) -> Option<(usize, Option<RequestSides>)> {
        self.0.upgrade()?.read(cx).request_branch(of)
    }

    fn set_list(&self, branches: Vec<BranchRef>, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| {
                let open = h
                    .branch
                    .and_then(|ix| h.branches.get(ix))
                    .map(|b| b.refname.clone());
                h.branches = Arc::new(branches);
                h.branch = open.and_then(|r| h.branches.iter().position(|b| b.refname == r));
            });
        }
    }

    fn show_request_commits(&self, base: &NamedTip, head: &NamedTip, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| {
                let id = |ix: usize| h.commits.get(ix).map(|c| c.id);
                let picked = match h.selection {
                    Selection::Commit(ix) => id(ix).map(|a| (a, a)),
                    Selection::Commits { newest, oldest } => id(newest).zip(id(oldest)),
                    _ => None,
                };
                h.show_request_commits(base, head);
                let at = |id: git2::Oid| h.commits.iter().position(|c| c.id == id);
                if let Some((a, b)) = picked
                    && let (Some(a), Some(b)) = (at(a), at(b))
                {
                    h.selection = match h.selection {
                        Selection::Commit(_) => Selection::Commit(a),
                        _ => Selection::Commits {
                            newest: a,
                            oldest: b,
                        },
                    }
                }
            });
        }
    }

    fn show_request_versions(&self, versions: Option<&(u64, Vec<Version>)>, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            h.update(cx, |h, _| h.show_request_versions(versions));
        }
    }

    fn on_change(&self, hear: std::rc::Rc<dyn Fn(&mut gpui::App)>, cx: &mut gpui::App) {
        if let Some(h) = self.0.upgrade() {
            cx.observe(&h, move |_, cx| hear(cx)).detach();
        }
    }

    fn diff_title(
        &self,
        route: Option<&artoo_core::route::Route>,
        cx: &gpui::App,
    ) -> Option<String> {
        let diff = self.review()?.shown(cx)?;
        let compare = route.and_then(|r| self.compare_title(r, cx));
        Some(crate::head::title(&diff, compare))
    }
}

/// The history's acts as its views ask them: what is its own — a version clicked, a section folded, the version
/// comparison's tab, a commit's message — it answers, the rest the tab (`tab`).
pub fn acts(history: &gpui::Entity<History>, tab: artoo_branches_api::TabActs) -> Acts {
    let tab = tab.0;
    let history = history.downgrade();
    std::rc::Rc::new(move |act, cx| {
        let Some(h) = history.upgrade() else {
            return tab(act, cx);
        };
        match act {
            Act::Version { ix, more } => {
                let (from, to) = h.read(cx).versions_clicked(ix, more);
                tab(Act::Versions { from, to }, cx)
            }
            Act::FindCommits => {
                if h.read(cx).sections.commits {
                    h.update(cx, |h, cx| h.fold(crate::panel::Fold::Commits, cx));
                }
                tab(Act::FindField, cx)
            }
            Act::Fold(fold) => h.update(cx, |h, cx| h.fold(fold, cx)),
            Act::VersionTab(version_tab) => h.update(cx, |h, cx| {
                h.version_tab = version_tab;
                cx.notify()
            }),
            Act::Message => h.update(cx, |h, cx| {
                h.message_open = !h.message_open;
                cx.notify()
            }),
            act => tab(act, cx),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A history whose list shows the commits `shown`, `selection` selected.
    fn listing(shown: Vec<usize>, selection: Selection) -> History {
        let mut h = History {
            selection,
            ..Default::default()
        };
        h.find.shown = Some(shown);
        h
    }

    #[test]
    fn the_arrows_walk_the_rows_shown_the_working_tree_over_them() {
        let h = listing(vec![1, 3, 4], Selection::Commit(3));
        assert_eq!(h.step_row(true, false), Some(Row::Commit(4)));
        assert_eq!(h.step_row(false, false), Some(Row::Commit(1)));
        let h = listing(vec![1, 3, 4], Selection::Commit(4));
        assert_eq!(h.step_row(true, false), None);
        let h = listing(vec![1, 3, 4], Selection::Commit(1));
        assert_eq!(h.step_row(false, false), None);
        assert_eq!(h.step_row(false, true), Some(Row::WorkingTree));
        let h = listing(vec![1, 3, 4], Selection::WorkingTree);
        assert_eq!(h.step_row(true, false), Some(Row::Commit(1)));
        assert_eq!(h.step_row(false, true), None);
        // Nothing selected, or one the search hides: ↓ the first shown.
        let h = listing(vec![1, 3, 4], Selection::None);
        assert_eq!(h.step_row(true, false), Some(Row::Commit(1)));
        assert_eq!(h.step_row(false, false), None);
        let h = listing(vec![1, 3, 4], Selection::Commit(2));
        assert_eq!(h.step_row(true, false), None);
        let h = listing(Vec::new(), Selection::WorkingTree);
        assert_eq!(h.step_row(true, true), None);
    }
}
