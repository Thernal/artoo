//! A route of a branch's or a request's kind opened (#165 I5d): what showing it takes, by what the history has open —
//! the branch read anew, or only a commit, a run, versions, its own Changes picked on the branch open; the working
//! tree; a comparison; a request opened (its branch read with it), or its whole diff, a commit, a run or versions of
//! it. A page that waits for its branch or its request to be read is kept as `waiting`. Which branch a request is
//! read from is the history's too (`request_branch`), and showing it (`show_route`); what the tab keeps of it and
//! a request's reading stay the tab's for now (`Shows`).

use crate::history::{History, NamedTip, RequestSides};
use crate::waiting::{Pick, Waiting};
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_core::git::{CommitInfo, RefKind, Repo};
use artoo_core::route::{Place, Route};
use artoo_requests_api::{
    RequestCommitRoute, RequestCommitsRoute, RequestRoute, RequestVersionsRoute,
};
use artoo_review_api::Review;
use git2::Oid;
use gpui::Context;

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
    /// Request `number` opened: its branch read with it.
    Request(u64),
    /// The request open's whole diff: its sides, as it is open (`open_request`).
    RequestChanges(RequestSides),
    /// Nothing: the branch it is of is not here, or nothing is listed yet.
    Stay,
}

/// What showing a route came to (`History::show_route`), for what the tab does after.
pub enum Shows {
    /// Branch `ix` opened, `read` anew when there is one to read; what follows its read is `show_read`'s.
    Branch { ix: usize, read: bool },
    /// Request `number` to open: what it is read from waits for the requests (the tab's for now).
    Request(u64),
    /// Shown: the commit it is, kept among the places opened lately; `follow` when the page on screen follows
    /// what is shown.
    Shown {
        commit: Option<CommitInfo>,
        follow: bool,
    },
    /// Nothing changed.
    Stay,
}

/// What a merge request is read from (`History::request_branch`): its own fields, as its host says them.
pub struct RequestOf<'a> {
    pub number: u64,
    /// The branch it is from, and the one it goes into.
    pub source: &'a str,
    pub target: &'a str,
    /// Its head, as the host has it.
    pub head: &'a str,
    /// Open, not merged nor closed.
    pub open: bool,
    /// The commit it was merged with, once it is.
    pub merged_with: Option<&'a str>,
    /// On GitHub, where a fork's pull request is fetched to `pr/<n>`.
    pub github: bool,
}

impl History {
    /// Route `route` shown (`opening`); `same` when its branch is the one open. `None` for a route of another
    /// kind.
    pub fn show_route(
        &mut self,
        route: &Route,
        same: bool,
        review: &dyn Review,
        cx: &mut Context<Self>,
    ) -> Option<Shows> {
        let shown = |follow| Shows::Shown {
            commit: None,
            follow,
        };
        Some(match self.opening(route, same)? {
            Opening::Branch { ix, keep } => Shows::Branch {
                ix,
                read: self.open_branch(ix, keep, review, cx),
            },
            Opening::Changes => {
                self.show_branch_compare(review, cx);
                shown(false)
            }
            Opening::Commit(ix) => match self.show_commit(ix, None, review, cx) {
                Some(commit) => Shows::Shown {
                    commit: Some(commit),
                    follow: true,
                },
                None => Shows::Stay,
            },
            Opening::Versions(from, to) => shown(self.show_versions(from, to, None, review, cx)),
            Opening::Finish => match self.finish_waiting(review, cx) {
                Some(finished) => Shows::Shown {
                    commit: finished.commit,
                    follow: true,
                },
                None => Shows::Stay,
            },
            Opening::WorkingTree => {
                let only_unstaged = self.wt.only_unstaged;
                self.show_working_tree(only_unstaged, None, review, cx);
                shown(false)
            }
            Opening::Compare(base, head, since) => {
                self.show_compare(base.clone(), head.clone(), since, None, review, cx);
                // A comparison's own page lists its commits (#21).
                self.read_compare((base, head, since), cx);
                shown(false)
            }
            Opening::Request(number) => Shows::Request(number),
            Opening::RequestChanges((base, head, number)) => {
                self.show_compare(base, head, true, Some(number), review, cx);
                // A request's commits are its own Commits.
                self.compare = None;
                shown(true)
            }
            Opening::Stay => Shows::Stay,
        })
    }

    /// What showing `route` takes; `same` when its branch is the one open (the item loaded). A branch's own page
    /// is its Changes when it has its own commits, else its newest commit: another commit is a page of its own (the
    /// developer, 2026-11-01). A run's or versions' page asked of a branch not open waits for it (`waiting`). `None`
    /// for a route of another kind.
    pub fn opening(&mut self, route: &Route, same: bool) -> Option<Opening> {
        if let Some(BranchRoute { refname }) = BranchRoute::from_route(route) {
            return Some(if same && self.tabbed(&refname) {
                Opening::Changes
            } else if same && self.commits.is_empty() {
                Opening::Stay
            } else if same {
                Opening::Commit(0)
            } else {
                self.reopen(&refname, None)
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
        if let Some(RequestRoute { number }) = RequestRoute::from_route(route) {
            return Some(match self.open_request.clone() {
                Some(sides) if same => Opening::RequestChanges(sides),
                _ if same => Opening::Stay,
                _ => Opening::Request(number),
            });
        }
        let request_pick = if let Some(r) = RequestCommitRoute::from_route(route) {
            Some((r.number, Pick::Commit(Oid::from_str(&r.commit).ok()?)))
        } else if let Some(r) = RequestCommitsRoute::from_route(route) {
            let (newest, oldest) = (
                Oid::from_str(&r.newest).ok()?,
                Oid::from_str(&r.oldest).ok()?,
            );
            Some((r.number, Pick::Commits(newest, oldest)))
        } else {
            RequestVersionsRoute::from_route(route)
                .map(|r| (r.number, Pick::Versions(r.from, r.to)))
        };
        if let Some((number, pick)) = request_pick {
            // A request's commit, run or versions: shown at once when the request is loaded, else once it is.
            self.waiting = Some(Waiting {
                request: Some(number),
                pick,
            });
            return Some(if same {
                Opening::Finish
            } else {
                Opening::Request(number)
            });
        }
        let r = CompareRoute::from_route(route)?;
        Some(Opening::Compare(
            (r.base, Oid::from_str(&r.base_commit).ok()?),
            (r.head, Oid::from_str(&r.head_commit).ok()?),
            r.since,
        ))
    }

    /// The branch request `m` is read from, and the sides it is compared on once it is (none while its target is
    /// not here): its source branch here — a remote one first —, a merged or closed one's only where it was left,
    /// else `pr/<n>` on GitHub, else its own ref. A merged one's base is the merge commit's first parent, so its
    /// diff is what it brought. `None` while none of them is here.
    pub fn request_branch(&self, m: &RequestOf) -> Option<(usize, Option<RequestSides>)> {
        let pick = |name: &str| {
            let remote = self.branches.iter().position(|b| {
                b.kind == RefKind::Remote && b.name.split_once('/').is_some_and(|(_, n)| n == name)
            });
            remote.or_else(|| {
                self.branches
                    .iter()
                    .position(|b| b.kind == RefKind::Local && b.name == name)
            })
        };
        let own = artoo_core::git::request_ref(m.number);
        let src = pick(m.source)
            // A merged or closed one's branch may have moved on since.
            .filter(|&ix| m.open || self.branches[ix].tip.to_string() == m.head)
            .or_else(|| {
                m.github
                    .then(|| pick(&format!("pr/{}", m.number)))
                    .flatten()
            })
            .or_else(|| self.branches.iter().position(|b| b.refname == own))?;
        let side = |ix: usize| (self.branches[ix].name.clone(), self.branches[ix].tip);
        let sides = pick(m.target).map(|tgt| {
            let mut base = side(tgt);
            if let Some(parent) = m
                .merged_with
                .and_then(|sha| Oid::from_str(sha).ok())
                .zip(self.root.as_deref())
                .and_then(|(id, root)| Repo::open(root).ok()?.parent_of(id).ok())
            {
                base.1 = parent;
            }
            (base, side(src), m.number)
        });
        Some((src, sides))
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
        // A branch without its own commits: its page is its newest commit.
        assert_eq!(h.opening(&branch(), true), Some(Opening::Commit(0)));
        assert_eq!(
            h.opening(&branch(), false),
            Some(Opening::Branch { ix: 0, keep: None })
        );
        let commit = BranchCommitRoute {
            refname: "refs/heads/feature".into(),
            commit: id(1).to_string(),
        }
        .route();
        assert_eq!(h.opening(&commit, true), Some(Opening::Commit(2)));
        let gone = BranchRoute {
            refname: "refs/heads/gone".into(),
        }
        .route();
        assert_eq!(h.opening(&gone, false), Some(Opening::Stay));
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
            h.opening(&run, false),
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
            h.opening(&versions, false),
            Some(Opening::Branch { ix: 0, keep: None })
        );
        assert_eq!(h.waiting.map(|w| w.pick), Some(Pick::Versions(0, 1)));
        let request = RequestRoute { number: 1 }.route();
        assert_eq!(h.opening(&request, false), Some(Opening::Request(1)));
        let other = Route {
            kind: "code.file".into(),
            args: r#"{"path":"a.rs"}"#.into(),
        };
        assert_eq!(h.opening(&other, false), None);
    }

    #[test]
    fn a_request_is_read_from_its_remote_branch_against_its_target() {
        let branch = |name: &str, kind: RefKind, n: u8| BranchRef {
            name: name.into(),
            refname: format!("refs/x/{name}"),
            kind,
            tip: id(n),
            is_head: false,
        };
        let h = History {
            branches: Arc::new(vec![
                branch("feature", RefKind::Local, 1),
                branch("origin/feature", RefKind::Remote, 2),
                branch("origin/main", RefKind::Remote, 3),
            ]),
            ..Default::default()
        };
        let mut m = RequestOf {
            number: 7,
            source: "feature",
            target: "main",
            head: "",
            open: true,
            merged_with: None,
            github: false,
        };
        let main = ("origin/main".to_owned(), id(3));
        let feature = ("origin/feature".to_owned(), id(2));
        assert_eq!(h.request_branch(&m), Some((1, Some((main, feature, 7)))));
        // A closed one's branch has moved on: not read from it.
        m.open = false;
        assert_eq!(h.request_branch(&m), None);
    }
}
