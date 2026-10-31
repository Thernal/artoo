//! Where the history is, as a route of the branches' (#165 I3): what the diff shows of the branch open — the branch,
//! a commit or a run of its commits, two versions, the working tree, a comparison —, which nav keeps and goes back to.

use crate::history::{History, Selection};
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_core::route::{Place, Route};
use artoo_review_api::diff::Header;

impl History {
    /// Branch `refname` is the one open and has the tabs Commits · Changes: it forks from the main line.
    pub fn tabbed(&self, refname: &str) -> bool {
        self.page
            .as_ref()
            .is_some_and(|f| f.refname == refname && f.base.is_some())
    }

    /// The diff `shown` is the branch page's own: the branch open against the main line.
    pub fn shows_branch(&self, shown: Option<&Header>) -> bool {
        let (Some(f), Some(ix)) = (self.page.as_ref(), self.branch) else {
            return false;
        };
        matches!(
            (shown, &f.base, self.branches.get(ix)),
            (
                Some(Header::Compare { base, head, request: None, .. }),
                Some(main),
                Some(b),
            ) if base.1 == main.1 && head.1 == b.tip
        )
    }

    /// Where the branch open is, by the selection and the diff `shown`: a commit of a branch with its own commits,
    /// not the main line's; the branch's own comparison is its page, another comparison its own. None while no
    /// branch is open, or a commit picked is gone.
    pub fn route(&self, shown: Option<&Header>) -> Option<Route> {
        let refname = self.branches.get(self.branch?)?.refname.clone();
        let hex = |id: git2::Oid| id.to_string();
        let tabbed = self.tabbed(&refname);
        Some(match (self.selection, shown) {
            (Selection::WorkingTree, _) => WorkingTreeRoute {}.route(),
            (Selection::Versions { from, to }, _) => VersionsRoute { refname, from, to }.route(),
            (Selection::Commit(ix), _) if tabbed => BranchCommitRoute {
                refname,
                commit: hex(self.commits.get(ix)?.id),
            }
            .route(),
            (Selection::Commits { newest, oldest }, _) => BranchCommitsRoute {
                refname,
                newest: hex(self.commits.get(newest)?.id),
                oldest: hex(self.commits.get(oldest)?.id),
            }
            .route(),
            (Selection::None, Some(Header::Compare { .. })) if self.shows_branch(shown) => {
                BranchRoute { refname }.route()
            }
            (
                Selection::None,
                Some(Header::Compare {
                    base,
                    head,
                    since_merge_base,
                    ..
                }),
            ) => CompareRoute {
                base: base.0.clone(),
                base_commit: hex(base.1),
                head: head.0.clone(),
                head_commit: hex(head.1),
                since: *since_merge_base,
            }
            .route(),
            _ => BranchRoute { refname }.route(),
        })
    }
}
