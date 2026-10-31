//! The branch open: the repository's branches, which one is open and what its page says of it, its versions and
//! its commits — a page of them, older ones read as the list asks —, and what of them the diff shows.

use crate::compare::CompareCommits;
use crate::facts::Facts;
use crate::find::Find;
use crate::reading::Read;
use crate::roots::Roots;
use artoo_core::git::{BranchRef, CommitInfo, Deco, Version};
use gpui::{EventEmitter, Task};
use std::collections::HashMap;
use std::sync::Arc;

/// A ref's name and the commit it points at.
pub type NamedTip = (String, git2::Oid);

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
    /// Labels on commits: where branches, remote branches and tags point.
    pub decor: HashMap<git2::Oid, Vec<Deco>>,
    /// The commits' search (⌘⇧F).
    pub find: Find,
    /// What the Branches list reads for its rows and previews (#72).
    pub roots: Roots,
    /// The branch being read; replacing it drops, and so cancels, the one it replaces.
    pub task: Option<Task<()>>,
    /// The branch read, until the tab shows it (`History::settle`).
    pub(crate) read: Option<Read>,
}

/// What the tab hears of the history's reads.
pub enum Event {
    /// The branch open was read: the tab shows it (`History::settle`).
    Read,
    /// More of its history is listed.
    Older,
    /// A read failed: what to say.
    Failed(String),
}

impl EventEmitter<Event> for History {}

/// What a click in the history's views asks of the tab.
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
}

/// How a view asks the tab an `Act`.
pub type Acts = std::rc::Rc<dyn Fn(Act, &mut gpui::App)>;

impl History {
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
}

impl Default for History {
    fn default() -> Self {
        History {
            branches: Arc::default(),
            branch: None,
            page: None,
            compare: None,
            versions: Vec::new(),
            commits: Vec::new(),
            older: None,
            selection: Selection::None,
            version_tab: VersionTab::Files,
            decor: HashMap::new(),
            find: Find::default(),
            roots: Roots::default(),
            task: None,
            read: None,
        }
    }
}
