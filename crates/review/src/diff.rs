//! The diff the review shows: what it is of (`Header`), its files, the version comparison's pairs and the working
//! tree's words; and a change to it the reviewer did not ask for (`Notice`).

use artoo_core::git::{CommitInfo, FileDiff, PairCommit, RangePair, Stale, Version};
use std::collections::HashSet;
use std::sync::Arc;

/// What a diff is of.
pub enum Header {
    Commit(CommitInfo),
    Versions {
        from: Version,
        to: Version,
        rebased: bool,
        conflicts: usize,
    },
    /// Uncommitted work against `HEAD`.
    WorkingTree {
        only_unstaged: bool,
    },
    /// Any two refs: what `head` changed against `base`.
    Compare {
        base: (String, git2::Oid),
        head: (String, git2::Oid),
        since_merge_base: bool,
        start: git2::Oid,
        commits: usize,
        /// The merge request this is the diff of, so comments can go to it.
        request: Option<u64>,
        /// The base is a local branch behind its upstream.
        stale: Option<Stale>,
    },
    /// One commit of an older version against its counterpart in a newer one.
    Interdiff {
        from: usize,
        to: usize,
        old: PairCommit,
        new: PairCommit,
    },
}

/// A change the user did not ask for, with the way back.
pub struct Notice {
    pub text: String,
    /// The versions compared before (from, to), to go back to.
    pub undo: Option<(usize, usize)>,
}

/// A diff to review: what it is of, and its files.
pub struct Diff {
    pub header: Header,
    pub files: Arc<Vec<FileDiff>>,
    /// The commits of a version comparison, paired (empty for anything else).
    pub pairs: Vec<RangePair>,
    /// The working tree's words for its files (staged, untracked…) and its two groups.
    pub tags: FileTags,
}

impl Diff {
    /// How many files: a file partly staged, in both of the working tree's groups (#22), is one.
    pub fn file_count(&self) -> usize {
        match self.tags.staged {
            Some(_) => self
                .files
                .iter()
                .map(FileDiff::path)
                .collect::<HashSet<_>>()
                .len(),
            None => self.files.len(),
        }
    }
}

/// The working tree's words for its files, by index: `staged`, `partly staged`, `not staged` or
/// `untracked`; empty for any other diff.
#[derive(Default)]
pub struct FileTags {
    pub words: Vec<&'static str>,
    /// In two groups (#22): how many of the files, first, are what is staged.
    pub staged: Option<usize>,
}

impl FileTags {
    pub fn word(&self, ix: usize) -> Option<&'static str> {
        self.words.get(ix).copied()
    }

    /// The word a row of the list shows: in a group only `untracked`, as its header says the rest.
    pub fn row_word(&self, ix: usize) -> Option<&'static str> {
        self.word(ix)
            .filter(|w| self.staged.is_none() || *w == "untracked")
    }
}
