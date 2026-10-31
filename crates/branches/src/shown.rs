//! What the diff shows of the history (#165 I1, I2): a commit, a run of them, two versions compared or an interdiff
//! of theirs, the working tree, a comparison or a run of its commits — the selection on it, its diff opened through
//! the review's contract, the message folded. Where the page goes and what it keeps of it (the history of pages, the places opened
//! lately, a comparison's own commits) stay the tab's.

use crate::history::{History, Selection, VersionTab};
use artoo_core::git::CommitInfo;
use artoo_review_api::{Review, Source};
use gpui::{App, ScrollStrategy};

impl History {
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
