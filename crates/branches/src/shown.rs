//! What the diff shows of the history (#165 I1, I2): a commit, a run of them, two versions compared or an interdiff
//! of theirs, the working tree, a comparison or a run of its commits — the selection on it, its diff opened through
//! the review's contract, the message folded —; a branch opened, what the one before showed forgotten. Where the
//! page goes and what it keeps of it (the history of pages, the places opened lately, a comparison's own commits)
//! stay the tab's.

use crate::history::{History, Selection, VersionTab};
use artoo_core::git::{CommitInfo, Version};
use artoo_review_api::{Review, Source};
use gpui::{App, Context, ScrollStrategy};

pub use artoo_branches_api::Settled;

impl History {
    /// The branch read shown (`settle`), its commits filtered and their paths indexed: a branch with its own commits
    /// shows the page waiting or its Changes; another the commit asked for or its newest, then the request that
    /// waited for its commits (`versions` the host's of it) — the page of it waiting, or its whole diff —, else the
    /// page waiting. None when nothing was read.
    pub fn show_read(
        &mut self,
        versions: Option<&(u64, Vec<Version>)>,
        review: &dyn Review,
        cx: &mut Context<Self>,
    ) -> Option<Settled> {
        let shown = self.settle()?;
        self.refilter();
        self.index_paths(cx);
        let mut commits = Vec::new();
        if shown.tabbed && shown.keep.is_none() {
            let finished = self.finish_waiting(review, cx);
            let follow = finished.is_some();
            match finished {
                Some(finished) => commits.extend(finished.commit),
                None => {
                    self.show_branch_compare(review, cx);
                }
            }
            return Some(Settled { commits, follow });
        }
        let ix = shown
            .keep
            .and_then(|id| self.commits.iter().position(|c| c.id == id))
            .unwrap_or(0);
        if !self.commits.is_empty() {
            commits.extend(self.show_commit(ix, None, review, cx));
        }
        let mut follow = !commits.is_empty();
        let pending = self.open_pending_request(versions);
        match self.finish_waiting(review, cx) {
            Some(finished) => {
                follow = true;
                commits.extend(finished.commit);
            }
            // A commit or versions of it waited for its commits (#71); else the whole request.
            None => {
                if let Some((base, head, iid)) = pending {
                    self.show_compare(base, head, true, Some(iid), review, cx);
                    self.compare = None;
                    follow = true;
                }
            }
        }
        Some(Settled { commits, follow })
    }

    /// The branch open against the main line since they parted: its Changes. Whether it has a base.
    pub fn show_branch_compare(&mut self, review: &dyn Review, cx: &mut App) -> bool {
        let (Some(ix), Some(base)) = (self.branch, self.page.as_ref().and_then(|f| f.base.clone()))
        else {
            return false;
        };
        let Some(b) = self.branches.get(ix) else {
            return false;
        };
        let head = (b.name.clone(), b.tip);
        self.show_compare(base, head, true, None, review, cx);
        // The comparison's own commits are its own page's; the branch's are its Commits.
        self.compare = None;
        true
    }

    /// Branch `ix` opened at commit `keep`, however old: no request on it until one opens it, what the branch
    /// before showed forgotten (`forget`), and it is read (`read_branch`). Whether there is such a branch.
    pub fn open_branch(
        &mut self,
        ix: usize,
        keep: Option<git2::Oid>,
        review: &dyn Review,
        cx: &mut Context<Self>,
    ) -> bool {
        self.open_request = None;
        if self.root.is_none() || self.branches.get(ix).is_none() {
            return false;
        }
        self.forget(review, cx);
        self.read_branch(ix, keep, cx);
        true
    }

    /// What the branch open showed is gone — its commits, versions and page —, the diff with it.
    pub fn forget(&mut self, review: &dyn Review, cx: &mut App) {
        self.branch = None;
        self.page = None;
        self.versions.clear();
        self.commits.clear();
        self.older = None;
        self.clear_diff(review, cx);
    }

    /// Nothing picked, the message folded, the diff gone.
    pub fn clear_diff(&mut self, review: &dyn Review, cx: &mut App) {
        self.message_open = false;
        self.selection = Selection::None;
        review.clear(cx);
    }

    /// Commit `ix` shown, its row in view, `keep` the file to open in it: the commit, when there is one.
    pub fn show_commit(
        &mut self,
        ix: usize,
        keep: Option<String>,
        review: &dyn Review,
        cx: &mut App,
    ) -> Option<CommitInfo> {
        let commit = self.commits.get(ix)?.clone();
        review.clear(cx);
        self.message_open = false;
        self.selection = Selection::Commit(ix);
        if let Some(row) = self.row_of(ix) {
            self.commit_scroll
                .scroll_to_item(row, ScrollStrategy::Nearest);
        }
        review.open(Source::Commit(commit.clone()), keep, cx);
        Some(commit)
    }

    /// A run of the commits, `newest` to `oldest` (indexes, newest first), shown as one diff: what the newest
    /// holds against the oldest's parent, as a request's Changes shows all its commits. Whether both are there.
    pub fn show_commits(
        &mut self,
        newest: usize,
        oldest: usize,
        keep: Option<String>,
        review: &dyn Review,
        cx: &mut App,
    ) -> bool {
        let (Some(head), Some(tail)) = (
            self.commits.get(newest).map(|c| c.id),
            self.commits.get(oldest).map(|c| c.id),
        ) else {
            return false;
        };
        review.clear(cx);
        self.message_open = false;
        self.selection = Selection::Commits { newest, oldest };
        let run = Source::Commits {
            newest: head,
            oldest: tail,
        };
        review.open(run, keep, cx);
        true
    }

    /// Versions `from` and `to` compared; asked anew (no `keep`), on their commits paired when the newer has more
    /// than one. Whether both are there.
    pub fn show_versions(
        &mut self,
        from: usize,
        to: usize,
        keep: Option<String>,
        review: &dyn Review,
        cx: &mut App,
    ) -> bool {
        let (Some(a), Some(b)) = (
            self.versions.get(from).cloned(),
            self.versions.get(to).cloned(),
        ) else {
            return false;
        };
        if keep.is_none() {
            self.version_tab = if b.commits > 1 {
                VersionTab::Commits
            } else {
                VersionTab::Files
            };
        }
        review.clear(cx);
        self.message_open = false;
        self.selection = Selection::Versions { from, to };
        review.open(Source::Versions { from: a, to: b }, keep, cx);
        true
    }

    /// The interdiff of pair `pair` of the versions compared: the old commit's patch against the new one's.
    /// Whether the pair has both.
    pub fn show_interdiff(&mut self, pair: usize, review: &dyn Review, cx: &mut App) -> bool {
        let Selection::Versions { from, to } = self.selection else {
            return false;
        };
        let Some((old, new)) = review
            .shown(cx)
            .and_then(|d| d.pairs.get(pair).cloned())
            .and_then(|p| Some((p.old?, p.new?)))
        else {
            return false;
        };
        review.clear(cx);
        self.message_open = false;
        review.open(Source::Interdiff { from, to, old, new }, None, cx);
        true
    }

    /// The working tree against `HEAD`, or what is not staged only, read again in place: the file open keeps its
    /// place, `keep` the one to open.
    pub fn show_working_tree(
        &mut self,
        only_unstaged: bool,
        keep: Option<String>,
        review: &dyn Review,
        cx: &mut App,
    ) {
        self.message_open = false;
        self.selection = Selection::WorkingTree;
        review.reopen(Source::WorkingTree { only_unstaged }, keep, cx);
    }

    /// What `head` changed against `base`, since their merge base or directly, a request's diff or not.
    pub fn show_compare(
        &mut self,
        base: (String, git2::Oid),
        head: (String, git2::Oid),
        since: bool,
        request: Option<u64>,
        review: &dyn Review,
        cx: &mut App,
    ) {
        review.clear(cx);
        self.message_open = false;
        self.selection = Selection::None;
        let compare = Source::Compare {
            base,
            head,
            since,
            request,
        };
        review.open(compare, None, cx);
    }

    /// The run of the comparison's commits picked, as one diff under the comparison's page: what the newest holds
    /// against the oldest's parent; `keep` the file to open. Whether a run is picked.
    pub fn show_compare_pick(
        &mut self,
        keep: Option<String>,
        review: &dyn Review,
        cx: &mut App,
    ) -> bool {
        let Some((newest, oldest)) = self.compare.as_ref().and_then(|c| c.picked_ids()) else {
            return false;
        };
        review.clear(cx);
        self.message_open = false;
        self.selection = Selection::None;
        review.open(Source::Commits { newest, oldest }, keep, cx);
        true
    }
}
