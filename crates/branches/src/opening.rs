//! A route of a branch's kind opened (#165 I5d): what showing it takes, by what the history has open — the branch
//! read anew, or only a commit, a run, versions, its own Changes picked on the branch open; the working tree; a
//! comparison. A page that waits for its branch to be read is kept as `waiting`. What the tab does with each, and
//! what it keeps of it, stays the tab's for now.

use crate::history::{History, NamedTip};
use crate::waiting::{Pick, Waiting};
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_core::route::{Place, Route};
use git2::Oid;

/// What the tab does to show a route of a branch's kind (`History::opening`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Opening {
    /// Branch `ix` read anew, `keep` the commit selected once it is.
    Branch {
        ix: usize,
        keep: Option<Oid>,
    },
    /// The branch open's own page: it against the main line since they parted.
    Changes,
    /// Commit `ix` of the branch open.
    Commit(usize),
    /// Two of the branch open's versions compared.
    Versions(usize, usize),
    /// What the page waits for is in: it is shown (`History::finish_waiting`).
    Finish,
    WorkingTree,
    /// Two refs compared, since their merge base or not.
    Compare(NamedTip, NamedTip, bool),
    /// Nothing: the branch it is of is not here, or nothing is listed yet.
    Stay,
}

impl History {
    /// What showing `route` takes; `same` when its branch is the one open (the item loaded), `remembered` the
    /// commit last shown on the branch's page. A run's or versions' page asked of a branch not open waits for it
    /// (`waiting`). `None` for a route of another kind.
    pub fn opening(
        &mut self,
        route: &Route,
        same: bool,
        remembered: Option<Oid>,
    ) -> Option<Opening> {
        if let Some(BranchRoute { refname }) = BranchRoute::from_route(route) {
            return Some(if same && self.tabbed(&refname) {
                Opening::Changes
            } else if same {
                let ix = remembered
                    .and_then(|id| self.commits.iter().position(|c| c.id == id))
                    .unwrap_or(0);
                if self.commits.is_empty() {
                    Opening::Stay
                } else {
                    Opening::Commit(ix)
                }
            } else {
                self.reopen(&refname, remembered)
            });
        }
        if let Some(BranchCommitRoute { refname, commit }) = BranchCommitRoute::from_route(route) {
            let id = Oid::from_str(&commit).ok()?;
            let at = self.commits.iter().position(|c| c.id == id);
            return Some(match at {
                Some(ix) if same => Opening::Commit(ix),
                _ => self.reopen(&refname, Some(id)),
            });
        }
        if let Some(BranchCommitsRoute {
            refname,
            newest,
            oldest,
        }) = BranchCommitsRoute::from_route(route)
        {
            let pick = Pick::Commits(Oid::from_str(&newest).ok()?, Oid::from_str(&oldest).ok()?);
            self.waiting = Some(Waiting {
                request: None,
                pick,
            });
            return Some(if same {
                Opening::Finish
            } else {
                self.reopen(&refname, None)
            });
        }
        if let Some(VersionsRoute { refname, from, to }) = VersionsRoute::from_route(route) {
            if same && to < self.versions.len() {
                return Some(Opening::Versions(from, to));
            }
            let opening = self.reopen(&refname, None);
            if opening != Opening::Stay {
                self.waiting = Some(Waiting {
                    request: None,
                    pick: Pick::Versions(from, to),
                });
            }
            return Some(opening);
        }
        if WorkingTreeRoute::from_route(route).is_some() {
            return Some(Opening::WorkingTree);
        }
        let r = CompareRoute::from_route(route)?;
        Some(Opening::Compare(
            (r.base, Oid::from_str(&r.base_commit).ok()?),
            (r.head, Oid::from_str(&r.head_commit).ok()?),
            r.since,
        ))
    }

    /// Branch `refname` read anew, `keep` selected once it is; `Stay` when it is not here.
    fn reopen(&self, refname: &str, keep: Option<Oid>) -> Opening {
        match self.branches.iter().position(|b| b.refname == refname) {
            Some(ix) => Opening::Branch { ix, keep },
            None => Opening::Stay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::git::{BranchRef, CommitInfo, RefKind};
    use std::sync::Arc;

    fn id(n: u8) -> Oid {
        Oid::from_bytes(&[n; 20]).unwrap()
    }

    fn commit(n: u8) -> CommitInfo {
        CommitInfo {
            id: id(n),
            summary: String::new(),
            message: String::new(),
            author: String::new(),
            email: String::new(),
            time: 0,
        }
    }

    /// A history listing `feature` with commits 3, 2, 1, newest first.
    fn history() -> History {
        History {
            branches: Arc::new(vec![BranchRef {
                name: "feature".into(),
                refname: "refs/heads/feature".into(),
                kind: RefKind::Local,
                tip: id(3),
                is_head: false,
            }]),
            commits: vec![commit(3), commit(2), commit(1)],
            ..Default::default()
        }
    }

    fn branch() -> Route {
        BranchRoute {
            refname: "refs/heads/feature".into(),
        }
        .route()
    }

    #[test]
    fn a_branch_open_shows_what_is_asked_one_not_open_is_read() {
        let mut h = history();
        assert_eq!(
            h.opening(&branch(), true, Some(id(2))),
            Some(Opening::Commit(1))
        );
        assert_eq!(h.opening(&branch(), true, None), Some(Opening::Commit(0)));
        assert_eq!(
            h.opening(&branch(), false, Some(id(2))),
            Some(Opening::Branch {
                ix: 0,
                keep: Some(id(2))
            })
        );
        let commit = BranchCommitRoute {
            refname: "refs/heads/feature".into(),
            commit: id(1).to_string(),
        }
        .route();
        assert_eq!(h.opening(&commit, true, None), Some(Opening::Commit(2)));
        let gone = BranchRoute {
            refname: "refs/heads/gone".into(),
        }
        .route();
        assert_eq!(h.opening(&gone, false, None), Some(Opening::Stay));
    }

    #[test]
    fn a_run_or_versions_of_a_branch_not_open_wait_for_it() {
        let mut h = history();
        let run = BranchCommitsRoute {
            refname: "refs/heads/feature".into(),
            newest: id(3).to_string(),
            oldest: id(2).to_string(),
        }
        .route();
        assert_eq!(
            h.opening(&run, false, None),
            Some(Opening::Branch { ix: 0, keep: None })
        );
        assert_eq!(h.waiting.map(|w| w.pick), Some(Pick::Commits(id(3), id(2))));
        let versions = VersionsRoute {
            refname: "refs/heads/feature".into(),
            from: 0,
            to: 1,
        }
        .route();
        h.waiting = None;
        assert_eq!(
            h.opening(&versions, false, None),
            Some(Opening::Branch { ix: 0, keep: None })
        );
        assert_eq!(h.waiting.map(|w| w.pick), Some(Pick::Versions(0, 1)));
        let other = Route {
            kind: "requests.request".into(),
            args: r#"{"number":1}"#.into(),
        };
        assert_eq!(h.opening(&other, false, None), None);
    }
}
