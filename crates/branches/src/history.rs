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

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    None,
    Commit(usize),
    /// A run of the list's commits as one diff: the newest's index and the oldest's.
    Commits {
        newest: usize,
        oldest: usize,
    },
    Versions {
        from: usize,
        to: usize,
    },
    WorkingTree,
}

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
    /// The commit the Commits page has picked (#73), and with shift the other end of a run of them
    /// (`commits_page`).
    pub pick: Option<git2::Oid>,
    pub pick_to: Option<git2::Oid>,
    /// Its lists' widths: the Commits section's (`side-commits`), the Commits page's (`commits`).
    pub widths: artoo_ui_kit::room::Widths,
}

/// What the tab hears of the history's reads.
pub enum Event {
    /// The branch open was read: the tab shows it (`History::settle`).
    Read,
    /// More of its history is listed.
    Older,
    /// ↵ in the commits' search: commit `ix`, the first found, shown.
    Picked(usize),
    /// A read failed: what to say.
    Failed(String),
}

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

impl artoo_branches_api::Branches for Handle {
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
