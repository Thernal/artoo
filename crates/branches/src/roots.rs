//! What the Branches list reads for its rows and previews (#72): each branch's facts, which remotes are open, the
//! working tree's files.

use artoo_core::git::ChangeKind;
use gpui::Task;
use std::collections::{HashMap, HashSet};

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
    pub facts_task: Option<Task<()>>,
    /// The remotes whose branches the list shows; the rest are folded to their name.
    pub open_remotes: HashSet<String>,
    /// The working tree's files as last read, with the fingerprint they were read at.
    pub tree: Option<(u64, Vec<TreeFile>)>,
    pub tree_task: Option<Task<()>>,
}
