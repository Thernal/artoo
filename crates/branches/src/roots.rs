//! What the Branches list reads for its rows and previews (#72): each branch's facts, which remotes are open, the
//! working tree's files — each read the history's own task —; and its rows, in the order it shows them.

use crate::history::History;
use artoo_core::git::{BranchRef, ChangeKind, DiffSettings, RefKind, Repo};
use artoo_core::rank;
use artoo_ui_kit::motion::{self, Folds, Marks};
use artoo_ui_kit::room::Heights;
use gpui::{Context, Task};
use std::collections::{BTreeMap, HashMap, HashSet};

pub use artoo_branches_api::BranchRow;

/// A branch, as its row and preview say it.
#[derive(Clone)]
pub struct Fact {
    /// When its tip was committed, its subject and who wrote it.
    pub time: i64,
    pub summary: String,
    pub author: String,
    /// What it is counted against (the main line, or its upstream for the main line itself) and
    /// how far apart: (name, ahead, behind).
    pub against: Option<(String, usize, usize)>,
}

/// A file of the working tree, for its row and preview.
#[derive(Clone)]
pub struct TreeFile {
    pub change: ChangeKind,
    pub path: String,
    pub added: usize,
    pub removed: usize,
}

#[derive(Default)]
pub struct Roots {
    /// Per refname.
    pub facts: HashMap<String, Fact>,
    facts_task: Option<Task<()>>,
    /// The remotes whose branches the list shows; the rest are folded to their name.
    pub open_remotes: HashSet<String>,
    /// How a remote's branches fold, in size and alpha (#110): its toggle's mark, what each fold was last drawn as,
    /// its height then.
    pub remote_motion: Marks,
    pub remote_folds: Folds,
    pub remote_heights: Heights,
    /// The row picked, previewed: by a click, by ↑ ↓, or the one open as you came back.
    pub picked: Option<BranchRow>,
    /// The working tree's files as last read, with the fingerprint they were read at.
    pub tree: Option<(u64, Vec<TreeFile>)>,
    tree_task: Option<Task<()>>,
}

impl History {
    /// Reads, in the background, when each branch's tip was made and how far it is from the main line.
    pub fn read_facts(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let branches = self.branches.clone();
        self.roots.facts_task = Some(cx.spawn(async move |this, cx| {
            let facts = cx
                .background_executor()
                .spawn(async move {
                    let repo = Repo::open(&root).ok()?;
                    let main = repo.default_branch();
                    let tip = |name: &str| branches.iter().find(|b| b.name == name).map(|b| b.tip);
                    let main_tip = main
                        .as_deref()
                        .and_then(|m| tip(m).or_else(|| tip(&format!("origin/{m}"))));
                    let facts: HashMap<String, Fact> = branches
                        .iter()
                        .filter_map(|b| {
                            let commit = repo.commit(b.tip).ok()?;
                            let against = match (b.kind, &main, main_tip) {
                                (RefKind::Remote, ..) => None,
                                (RefKind::Local, Some(m), _) if *m == b.name => repo.tracking(m),
                                (RefKind::Local, Some(m), Some(t)) => repo
                                    .ahead_behind(b.tip, t)
                                    .map(|(ahead, behind)| (m.clone(), ahead, behind)),
                                _ => None,
                            };
                            let fact = Fact {
                                time: commit.time,
                                summary: commit.summary,
                                author: commit.author,
                                against,
                            };
                            Some((b.refname.clone(), fact))
                        })
                        .collect();
                    Some(facts)
                })
                .await;
            this.update(cx, |h, cx| {
                if let Some(facts) = facts {
                    h.roots.facts = facts;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Reads the working tree's files again when they changed since the last read (`wt`).
    pub fn read_tree(&mut self, settings: DiffSettings, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let (count, hash) = (self.wt.count, self.wt.hash);
        if count == 0 || self.roots.tree.as_ref().is_some_and(|t| t.0 == hash) {
            return;
        }
        self.roots.tree_task = Some(cx.spawn(async move |this, cx| {
            let files = cx
                .background_executor()
                .spawn(async move {
                    let tree = Repo::open(&root)?.working_tree(settings, false)?;
                    anyhow::Ok(
                        tree.files
                            .iter()
                            .map(|f| TreeFile {
                                change: f.change,
                                path: f.path().to_owned(),
                                added: f.added,
                                removed: f.removed,
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .await;
            this.update(cx, |h, cx| {
                if let Ok(files) = files {
                    h.roots.tree = Some((hash, files));
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// A remote's branches listed, or folded to its name; they move as they fold (#110).
    pub fn toggle_remote(&mut self, remote: String, cx: &mut Context<Self>) {
        self.roots.remote_motion.mark(remote_key(&remote));
        motion::redraw_after(cx);
        if !self.roots.open_remotes.remove(&remote) {
            self.roots.open_remotes.insert(remote);
        }
        cx.notify();
    }
}

/// Remote `remote`'s fold in the list: its mark, and the key its height is kept by.
pub fn remote_key(remote: &str) -> String {
    format!("remote-{remote}")
}

impl History {
    /// The branches a search lists, by index: a request's fetched head is none of them (#114).
    pub fn listed(&self) -> impl Iterator<Item = (usize, &BranchRef)> {
        self.branches
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind != RefKind::Request)
    }

    /// Branches' rows in the order the list shows them: the working tree, local branches, then
    /// the branches of each open remote; a `filter` lists the matching branches only.
    pub fn rows(&self, filter: &str) -> Vec<BranchRow> {
        let filter = filter.to_lowercase();
        if !filter.trim().is_empty() {
            return rank::rank(self.listed(), &filter, |(_, b)| &b.name)
                .into_iter()
                .map(|(ix, _)| BranchRow::Branch(ix))
                .collect();
        }
        let mut out = vec![BranchRow::WorkingTree];
        out.extend(
            self.branches
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == RefKind::Local)
                .map(|(ix, _)| BranchRow::Branch(ix)),
        );
        for (remote, ixs) in self.remote_branches() {
            if self.roots.open_remotes.contains(&remote) {
                out.extend(ixs.into_iter().map(BranchRow::Branch));
            }
        }
        out
    }

    /// The remote branches, by remote.
    pub fn remote_branches(&self) -> BTreeMap<String, Vec<usize>> {
        let mut out: BTreeMap<String, Vec<usize>> = Default::default();
        for (ix, b) in self.branches.iter().enumerate() {
            if b.kind == RefKind::Remote {
                let remote = b.name.split_once('/').map_or("", |(r, _)| r);
                out.entry(remote.to_owned()).or_default().push(ix);
            }
        }
        out
    }

    /// "4 local · 6 on origin": the count after Branches' name in the bar.
    pub fn count_line(&self) -> String {
        let local = self
            .branches
            .iter()
            .filter(|b| b.kind == RefKind::Local)
            .count();
        let mut parts = vec![format!("{local} local")];
        parts.extend(
            self.remote_branches()
                .into_iter()
                .map(|(remote, ixs)| format!("{} on {remote}", ixs.len())),
        );
        parts.join(" · ")
    }

    /// The row Branches selects when nothing is: the working tree with changes, else the branch
    /// checked out.
    pub fn first_row(&self) -> Option<BranchRow> {
        if self.wt.count > 0 {
            return Some(BranchRow::WorkingTree);
        }
        self.branches
            .iter()
            .position(|b| b.is_head)
            .or(self.branch)
            .map(BranchRow::Branch)
    }
}
