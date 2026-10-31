//! The review's contract (#165 stage 4): what a page asks of the tab's review. What a diff is of (`Source`): a
//! commit, a run of commits, two versions, an interdiff, the working tree, a comparison — the review reads it. The
//! file Code reads (#71, #75): read at a revision, which one is read, closed. The review draws it; the page puts it
//! where it goes. And how far your review of a merge request got (`Seen`), which the requests list groups by.
//!
//! The tab's review offers it as `Rc<dyn FileView>` in the tab's services (`Frame::services`). What it changes is
//! changed at once: ask it from anywhere but the review's own update.

use artoo_core::git::{CommitInfo, PairCommit, Version};
use git2::Oid;
use gpui::App;
use std::path::PathBuf;

/// What a diff the review shows is of; it reads it from the repository with its own root and settings.
pub enum Source {
    /// A commit against its first parent.
    Commit(CommitInfo),
    /// A run of commits, newest and oldest: what the newest holds against the oldest's parent — one commit against
    /// its first parent —, under a comparison's header.
    Commits { newest: Oid, oldest: Oid },
    /// Two versions of a branch compared, their commits paired.
    Versions { from: Version, to: Version },
    /// One commit of an older version against its counterpart in a newer one, of the versions `from` and `to`.
    Interdiff {
        from: usize,
        to: usize,
        old: PairCommit,
        new: PairCommit,
    },
    /// What is not committed: all of it as `git status` groups it, or what is not staged only.
    WorkingTree { only_unstaged: bool },
    /// What `head` changed against `base`, each a name and the commit it was at, since their merge base or
    /// directly; the merge request it is the diff of, so comments can go to it.
    Compare {
        base: (String, Oid),
        head: (String, Oid),
        since: bool,
        request: Option<u64>,
    },
}

/// The file a page reads through the tab's review.
pub trait FileView {
    /// File `path` of project `root` read at `rev` (None: as it is on disk), in place of the one read.
    fn read(&self, root: PathBuf, path: String, rev: Option<Oid>, cx: &mut App);

    /// The path of the file read, when one is.
    fn path(&self, cx: &App) -> Option<String>;

    /// The file read is let go: nothing is read.
    fn close(&self, cx: &mut App);
}

/// How far your review of a merge request got, against its head now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seen {
    /// No file of it was ever marked.
    Never,
    /// Files were marked at this head.
    Started,
    /// Files were marked at an earlier head: it was pushed since.
    Earlier,
    /// Its whole diff at this head was reviewed.
    Done,
}
