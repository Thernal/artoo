//! The app's pages as routes (#165 stage 4, `artoo_core::route`): a kind and its fields for each, which
//! `nav.txt` keeps. Each type is its owner's contract's: Code's are in `artoo-code-api`, a branch's, a comparison's
//! and the working tree's in `artoo-branches-api`, a merge request's in `artoo-requests-api`.

use super::nav::Page;
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_code_api::{FileRoute, FolderRoute};
use artoo_core::route::{Place, Route};
use artoo_requests_api::{
    RequestCommitRoute, RequestCommitsRoute, RequestRoute, RequestVersionsRoute,
};
use git2::Oid;

impl Page {
    pub fn route(&self) -> Route {
        let hex = |id: &Oid| id.to_string();
        match self {
            Page::File(path) => FileRoute { path: path.clone() }.route(),
            Page::Folder(path) => FolderRoute { path: path.clone() }.route(),
            Page::Branch(refname) => BranchRoute {
                refname: refname.clone(),
            }
            .route(),
            Page::BranchCommit(refname, id) => BranchCommitRoute {
                refname: refname.clone(),
                commit: hex(id),
            }
            .route(),
            Page::BranchCommits(refname, newest, oldest) => BranchCommitsRoute {
                refname: refname.clone(),
                newest: hex(newest),
                oldest: hex(oldest),
            }
            .route(),
            Page::Versions(refname, from, to) => VersionsRoute {
                refname: refname.clone(),
                from: *from,
                to: *to,
            }
            .route(),
            Page::WorkingTree => WorkingTreeRoute {}.route(),
            Page::Compare((base, a), (head, b), since) => CompareRoute {
                base: base.clone(),
                base_commit: hex(a),
                head: head.clone(),
                head_commit: hex(b),
                since: *since,
            }
            .route(),
            Page::Request(number) => RequestRoute { number: *number }.route(),
            Page::RequestCommit(number, id) => RequestCommitRoute {
                number: *number,
                commit: hex(id),
            }
            .route(),
            Page::RequestCommits(number, newest, oldest) => RequestCommitsRoute {
                number: *number,
                newest: hex(newest),
                oldest: hex(oldest),
            }
            .route(),
            Page::RequestVersions(number, from, to) => RequestVersionsRoute {
                number: *number,
                from: *from,
                to: *to,
            }
            .route(),
        }
    }

    /// The page `route` is; `None` for a kind the app does not draw, or fields that do not fit it.
    pub fn from_route(route: &Route) -> Option<Page> {
        let oid = |hex: &str| Oid::from_str(hex).ok();
        Some(match route.kind.as_str() {
            FileRoute::KIND => Page::File(FileRoute::from_route(route)?.path),
            FolderRoute::KIND => Page::Folder(FolderRoute::from_route(route)?.path),
            BranchRoute::KIND => Page::Branch(BranchRoute::from_route(route)?.refname),
            BranchCommitRoute::KIND => {
                let r = BranchCommitRoute::from_route(route)?;
                Page::BranchCommit(r.refname, oid(&r.commit)?)
            }
            BranchCommitsRoute::KIND => {
                let r = BranchCommitsRoute::from_route(route)?;
                Page::BranchCommits(r.refname, oid(&r.newest)?, oid(&r.oldest)?)
            }
            VersionsRoute::KIND => {
                let r = VersionsRoute::from_route(route)?;
                Page::Versions(r.refname, r.from, r.to)
            }
            WorkingTreeRoute::KIND => {
                WorkingTreeRoute::from_route(route)?;
                Page::WorkingTree
            }
            CompareRoute::KIND => {
                let r = CompareRoute::from_route(route)?;
                Page::Compare(
                    (r.base, oid(&r.base_commit)?),
                    (r.head, oid(&r.head_commit)?),
                    r.since,
                )
            }
            RequestRoute::KIND => Page::Request(RequestRoute::from_route(route)?.number),
            RequestCommitRoute::KIND => {
                let r = RequestCommitRoute::from_route(route)?;
                Page::RequestCommit(r.number, oid(&r.commit)?)
            }
            RequestCommitsRoute::KIND => {
                let r = RequestCommitsRoute::from_route(route)?;
                Page::RequestCommits(r.number, oid(&r.newest)?, oid(&r.oldest)?)
            }
            RequestVersionsRoute::KIND => {
                let r = RequestVersionsRoute::from_route(route)?;
                Page::RequestVersions(r.number, r.from, r.to)
            }
            _ => return None,
        })
    }
}
