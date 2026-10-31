//! The diff the review shows: what it is of (`Header`), its files, the version comparison's pairs and the working
//! tree's words; and a change to it the reviewer did not ask for (`Notice`).

use artoo_core::git::{
    CommitInfo, DiffSettings, FileDiff, PairCommit, RangePair, Repo, Stale, Version,
};
use artoo_review_api::Source;
use artoo_ui_kit::format;
use std::collections::HashSet;
use std::path::Path;
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
    /// What `source` is, read from the repository at `root` with `settings`.
    pub fn read(root: &Path, source: Source, settings: DiffSettings) -> anyhow::Result<Diff> {
        let repo = Repo::open(root)?;
        let plain = |header, files| Diff {
            header,
            files: Arc::new(files),
            pairs: Vec::new(),
            tags: Default::default(),
        };
        Ok(match source {
            Source::Commit(commit) => {
                let files = repo.commit_diff(commit.id, settings)?;
                plain(Header::Commit(commit), files)
            }
            Source::Commits { newest, oldest } => {
                // A first commit has no parent: its diff is all it adds, a run is then from it.
                let base = repo.parent_of(oldest).unwrap_or(oldest);
                let (files, start, commits) = if newest == oldest {
                    (repo.commit_diff(newest, settings)?, base, 1)
                } else {
                    let c = repo.compare(base, newest, false, settings)?;
                    (c.files, c.start, c.commits)
                };
                let header = Header::Compare {
                    base: (format!("{}^", format::short(oldest)), base),
                    head: (format::short(newest), newest),
                    since_merge_base: false,
                    start,
                    commits,
                    request: None,
                    stale: None,
                };
                plain(header, files)
            }
            Source::Versions { from, to } => {
                let diff = repo.version_diff(&from, &to, settings)?;
                let pairs = repo.range_pairs(&from, &to).unwrap_or_default();
                Diff {
                    header: Header::Versions {
                        from,
                        to,
                        rebased: diff.rebased,
                        conflicts: diff.conflicts.len(),
                    },
                    files: Arc::new(diff.files),
                    pairs,
                    tags: Default::default(),
                }
            }
            Source::Interdiff { from, to, old, new } => {
                let diff = repo.interdiff(old.id, new.id, settings)?;
                plain(Header::Interdiff { from, to, old, new }, diff.files)
            }
            Source::WorkingTree { only_unstaged } => {
                // All changes as `git status` groups them (#22); Not staged only is that group alone.
                let tree = match only_unstaged {
                    true => repo.working_tree(settings, true)?,
                    false => repo.working_groups(settings)?,
                };
                Diff {
                    header: Header::WorkingTree { only_unstaged },
                    files: Arc::new(tree.files),
                    pairs: Vec::new(),
                    tags: FileTags {
                        words: tree.tags,
                        staged: tree.staged,
                    },
                }
            }
            Source::Compare {
                base,
                head,
                since,
                request,
            } => {
                let c = repo.compare(base.1, head.1, since, settings)?;
                // Only a local branch that still points where it was picked can be stale.
                let stale = repo
                    .stale(&base.0)
                    .filter(|_| repo.resolve(&base.0).ok() == Some(base.1));
                let header = Header::Compare {
                    base,
                    head,
                    since_merge_base: since,
                    start: c.start,
                    commits: c.commits,
                    request,
                    stale,
                };
                plain(header, c.files)
            }
        })
    }

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
