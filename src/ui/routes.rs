//! The app's pages as routes (#165 stage 4, `artoo_core::route`): a kind and its fields for each, which
//! `nav.txt` keeps. Each type moves to its owner's contract when its pages move out of the app.

use super::nav::Page;
use artoo_core::route::{Place, Route};
use git2::Oid;
use serde::{Deserialize, Serialize};

/// A file read in Code.
#[derive(Serialize, Deserialize)]
pub(crate) struct FileRoute {
    pub path: String,
}

impl Place for FileRoute {
    const KIND: &'static str = "code.file";
}

/// A folder of the project in Code.
#[derive(Serialize, Deserialize)]
pub(crate) struct FolderRoute {
    pub path: String,
}

impl Place for FolderRoute {
    const KIND: &'static str = "code.folder";
}

/// A branch, by its refname.
#[derive(Serialize, Deserialize)]
pub(crate) struct BranchRoute {
    pub refname: String,
}

impl Place for BranchRoute {
    const KIND: &'static str = "branches.branch";
}

/// A commit of a branch.
#[derive(Serialize, Deserialize)]
pub(crate) struct BranchCommitRoute {
    pub refname: String,
    pub commit: String,
}

impl Place for BranchCommitRoute {
    const KIND: &'static str = "branches.commit";
}

/// A run of a branch's commits, newest and oldest.
#[derive(Serialize, Deserialize)]
pub(crate) struct BranchCommitsRoute {
    pub refname: String,
    pub newest: String,
    pub oldest: String,
}

impl Place for BranchCommitsRoute {
    const KIND: &'static str = "branches.commits";
}

/// Two versions of a branch compared.
#[derive(Serialize, Deserialize)]
pub(crate) struct VersionsRoute {
    pub refname: String,
    pub from: usize,
    pub to: usize,
}

impl Place for VersionsRoute {
    const KIND: &'static str = "branches.versions";
}

/// What is not committed.
#[derive(Serialize, Deserialize)]
pub(crate) struct WorkingTreeRoute {}

impl Place for WorkingTreeRoute {
    const KIND: &'static str = "branches.working-tree";
}

/// Two refs compared, each with the commit it was at; since their merge base or directly.
#[derive(Serialize, Deserialize)]
pub(crate) struct CompareRoute {
    pub base: String,
    pub base_commit: String,
    pub head: String,
    pub head_commit: String,
    pub since: bool,
}

impl Place for CompareRoute {
    const KIND: &'static str = "branches.compare";
}

/// A merge request, by its number.
#[derive(Serialize, Deserialize)]
pub(crate) struct RequestRoute {
    pub number: u64,
}

impl Place for RequestRoute {
    const KIND: &'static str = "requests.request";
}

/// A commit of a merge request.
#[derive(Serialize, Deserialize)]
pub(crate) struct RequestCommitRoute {
    pub number: u64,
    pub commit: String,
}

impl Place for RequestCommitRoute {
    const KIND: &'static str = "requests.commit";
}

/// A run of a merge request's commits, newest and oldest.
#[derive(Serialize, Deserialize)]
pub(crate) struct RequestCommitsRoute {
    pub number: u64,
    pub newest: String,
    pub oldest: String,
}

impl Place for RequestCommitsRoute {
    const KIND: &'static str = "requests.commits";
}

/// Two versions of a merge request compared.
#[derive(Serialize, Deserialize)]
pub(crate) struct RequestVersionsRoute {
    pub number: u64,
    pub from: usize,
    pub to: usize,
}

impl Place for RequestVersionsRoute {
    const KIND: &'static str = "requests.versions";
}

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
