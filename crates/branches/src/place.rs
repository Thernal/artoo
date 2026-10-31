//! Where the history is, as a route of the branches' (#165 I3) or a request's (I5d): what the diff shows of the
//! branch open — the branch, a commit or a run of its commits, two versions, the working tree, a comparison —, or of
//! the request on it, which nav keeps and goes back to.

use crate::history::{History, Selection};
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_core::route::{Place, Route};
use artoo_requests_api::{
    RequestCommitRoute, RequestCommitsRoute, RequestRoute, RequestVersionsRoute,
};
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

    /// Where what is loaded is, as a route: the page of the request on the branch open while it is request
    /// `current`, by the selection; the comparison `on_screen` while a run of its commits narrows it (a filter of
    /// its page, not a page, #21); else where the branch is (`branch_route`). None while no branch is open.
    pub fn loaded_route(
        &self,
        shown: Option<&Header>,
        current: Option<u64>,
        on_screen: Option<&Route>,
    ) -> Option<Route> {
        self.route_at(self.selection, shown, current, on_screen)
    }

    /// The page `selection` of the branch open is once shown (`loaded_route`): a commit, a run or two versions, of
    /// the request open while it is `current`. None for a selection that is not one of those.
    pub fn route_of(&self, selection: Selection, current: Option<u64>) -> Option<Route> {
        match selection {
            Selection::Commit(_) | Selection::Commits { .. } | Selection::Versions { .. } => {
                self.route_at(selection, None, current, None)
            }
            Selection::None | Selection::WorkingTree => None,
        }
    }

    fn route_at(
        &self,
        selection: Selection,
        shown: Option<&Header>,
        current: Option<u64>,
        on_screen: Option<&Route>,
    ) -> Option<Route> {
        self.branches.get(self.branch?)?;
        if let Some(number) = self.open_request.as_ref().map(|r| r.2)
            && current == Some(number)
        {
            let hex = |ix: usize| Some(self.commits.get(ix)?.id.to_string());
            return Some(match selection {
                Selection::Commit(ix) => RequestCommitRoute {
                    number,
                    commit: hex(ix)?,
                }
                .route(),
                Selection::Commits { newest, oldest } => RequestCommitsRoute {
                    number,
                    newest: hex(newest)?,
                    oldest: hex(oldest)?,
                }
                .route(),
                Selection::Versions { from, to } => {
                    RequestVersionsRoute { number, from, to }.route()
                }
                Selection::None | Selection::WorkingTree => RequestRoute { number }.route(),
            });
        }
        if let Some(route) = on_screen.filter(|r| {
            selection == Selection::None && self.compare_of(r).is_some_and(|c| c.picked.is_some())
        }) {
            return Some(route.clone());
        }
        self.branch_route(selection, shown)
    }

    /// Where the branch open is, by `selection` and the diff `shown`: a commit of it — on a branch without its
    /// own commits any but the newest, which is the branch's own page there (the developer, 2026-11-01) —; the
    /// branch's own comparison is its page, another comparison its own. None while no branch is open, or a commit
    /// picked is gone.
    fn branch_route(&self, selection: Selection, shown: Option<&Header>) -> Option<Route> {
        let refname = self.branches.get(self.branch?)?.refname.clone();
        let hex = |id: git2::Oid| id.to_string();
        let tabbed = self.tabbed(&refname);
        Some(match (selection, shown) {
            (Selection::WorkingTree, _) => WorkingTreeRoute {}.route(),
            (Selection::Versions { from, to }, _) => VersionsRoute { refname, from, to }.route(),
            (Selection::Commit(ix), _) if tabbed || ix > 0 => BranchCommitRoute {
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
