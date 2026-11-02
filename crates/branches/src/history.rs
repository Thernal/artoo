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

/// A ref's name and the commit it points at.
pub type NamedTip = (String, git2::Oid);

/// A merge request on the branch open: its base and head, and its number.
pub type RequestSides = (NamedTip, NamedTip, u64);

pub use artoo_branches_api::Selection;

/// What the version comparison shows: the files that changed, or the commits paired up.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum VersionTab {
    Files,
    Commits,
}

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
    /// The repository read again after it changed (`reload`).
    pub reload: Option<Task<()>>,
    /// The commit the Commits page has picked (#73), and with shift the other end of a run of them
    /// (`commits_page`).
    pub pick: Option<git2::Oid>,
    pub pick_to: Option<git2::Oid>,
    /// Its lists' widths: the Commits section's (`side-commits`), the Commits page's (`commits`).
    pub widths: artoo_ui_kit::room::Widths,
}

pub use artoo_branches_api::Event;

impl EventEmitter<Event> for History {}

/// What a click in the history's views asks of the tab.
#[derive(Clone)]
pub enum Act {
    /// Version `ix` clicked, with ⌘ (`more`) or not (`History::versions_clicked`).
    Version { ix: usize, more: bool },
    /// Commit `ix` clicked: its diff, the drawer gone.
    Commit(usize),
    /// A right click on commit `ix` at `at`: its menu.
    CommitMenu {
        ix: usize,
        at: gpui::Point<gpui::Pixels>,
    },
    /// A comparison's commit `ix` clicked, with ⇧ (`span`) or not (`CompareCommits::pick`).
    PickCompare { ix: usize, span: bool },
    /// The comparison whole again: the "only" pill's ×.
    WholeCompare,
    /// The working tree's chip: its diff against HEAD.
    WorkingTree,
    /// The search's chip: the commits' search, their section open.
    FindCommits,
    /// A click in the search's field: it takes the keyboard.
    FindField,
    /// A section of the side panel folded or opened.
    Fold(crate::panel::Fold),
    /// A press in the Commits section: its list takes the keyboard (↑ ↓).
    CommitsZone,
    /// A right click on the branch line at `at`: branch `ix`'s menu.
    BranchMenu {
        ix: usize,
        at: gpui::Point<gpui::Pixels>,
    },
    /// A row of Branches clicked, `clicks` times: picked, or opened.
    PickRow {
        row: crate::roots::BranchRow,
        clicks: usize,
    },
    /// A row of Branches opened: the working tree, or the branch.
    OpenRow(crate::roots::BranchRow),
    /// A branch's request, by its number.
    ShowRequest(u64),
    /// A remote's branches folded or open.
    ToggleRemote(String),
    /// The working tree's halves: what is not staged only, or all of it.
    OnlyUnstaged(bool),
    /// What `head` changed against `base`, since their merge base or directly, a request's diff or not.
    Compare {
        base: (String, git2::Oid),
        head: (String, git2::Oid),
        since: bool,
        request: Option<u64>,
    },
    /// Versions `from` and `to` compared: an interdiff's way back to their commits.
    Versions { from: usize, to: usize },
    /// The version comparison's tab: its files, or its commits paired.
    VersionTab(VersionTab),
    /// A commit's message open under its title, or folded.
    Message,
    /// The pill's newer commits listed.
    ShowNew,
    /// Text to the clipboard: a commit's id.
    Copy(String),
    /// A page on the host: a commit's.
    OpenUrl(String),
}

/// How a view asks the tab an `Act`.
pub type Acts = std::rc::Rc<dyn Fn(Act, &mut gpui::App)>;

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
            reload: None,
            pick: None,
            pick_to: None,
            widths: Default::default(),
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

    fn on_event(&self, hear: std::rc::Rc<dyn Fn(&Event, &mut gpui::App)>, cx: &mut gpui::App) {
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
