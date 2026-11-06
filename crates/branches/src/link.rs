//! What the branches' pages are on the host (#162): a branch's page, a commit's, a run of a branch's commits as one
//! diff, two refs compared. A branch only on this Mac, a commit only here, a branch's earlier versions and what is
//! not committed have none, and say why.

use crate::history::History;
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_core::git::{WebRemote, on_host};
use artoo_core::route::{Place, Route};
use artoo_shell_api::Link;
use artoo_ui_kit::format;
use git2::Oid;

/// What a branch's page is, in ⌘K's Copy link to…; the space's own page, the list, has none.
pub(crate) fn called(route: Option<&Route>) -> &'static str {
    match route.map(|r| r.kind.as_str()) {
        Some(BranchRoute::KIND) => "this branch",
        Some(BranchCommitRoute::KIND) => "this commit",
        Some(BranchCommitsRoute::KIND) => "these commits",
        Some(VersionsRoute::KIND) => "these versions",
        Some(CompareRoute::KIND) => "this comparison",
        _ => "this page",
    }
}

/// Page `route` of the branches on host `web`, as history `h` has them; with `diff`, where the host shows its
/// diff: a branch's page shows none.
pub(crate) fn link(
    route: Option<&Route>,
    web: &WebRemote,
    diff: bool,
    h: &History,
) -> Result<Link, String> {
    let nothing = || "Nothing here has a link: open a branch or a merge request".to_owned();
    let route = route.ok_or_else(nothing)?;
    let oid = |hex: &str| Oid::from_str(hex).map_err(|_| nothing());
    match route.kind.as_str() {
        BranchRoute::KIND if diff => Err(nothing()),
        BranchRoute::KIND => {
            let refname = BranchRoute::from_route(route).ok_or_else(nothing)?.refname;
            let branch = h.branches.iter().find(|b| b.refname == refname);
            let name = on_host(
                &h.branches,
                branch.map_or(refname.as_str(), |b| b.name.as_str()),
            )?;
            Ok(Link {
                url: web.tree(&name, ""),
                what: format!("branch {name}"),
            })
        }
        BranchCommitRoute::KIND => {
            let sha = oid(&BranchCommitRoute::from_route(route)
                .ok_or_else(nothing)?
                .commit)?;
            if !h.pushed(sha) {
                return Err(format!(
                    "Commit {} is only on this Mac: push it for a link",
                    format::short(sha)
                ));
            }
            Ok(Link {
                url: web.commit(&sha.to_string()),
                what: format!("commit {}", format::short(sha)),
            })
        }
        BranchCommitsRoute::KIND => {
            let r = BranchCommitsRoute::from_route(route).ok_or_else(nothing)?;
            let (newest, oldest) = (oid(&r.newest)?, oid(&r.oldest)?);
            run(web, newest, oldest, h)
        }
        // What the branch was before a rewrite is this Mac's alone: the host keeps the newest.
        VersionsRoute::KIND => {
            Err("A branch's earlier versions are only on this Mac: no link".to_owned())
        }
        WorkingTreeRoute::KIND => Err("What is not committed is only on this Mac".to_owned()),
        CompareRoute::KIND => {
            let r = CompareRoute::from_route(route).ok_or_else(nothing)?;
            let (base, head) = (
                on_host(&h.branches, &r.base)?,
                on_host(&h.branches, &r.head)?,
            );
            let dots = if r.since { "..." } else { ".." };
            Ok(Link {
                url: web.compare(&base, &head, !r.since),
                what: format!("{base}{dots}{head}"),
            })
        }
        _ => Err(nothing()),
    }
}

/// A branch's commits from `oldest` to `newest` (a run picked with shift) as one diff on the host: the comparison of
/// the commit before the run with the newest.
fn run(web: &WebRemote, newest: Oid, oldest: Oid, h: &History) -> Result<Link, String> {
    let range = format!("{}..{}", format::short(oldest), format::short(newest));
    if !h.pushed(newest) {
        return Err(format!(
            "Commits {range} are only on this Mac: push them for a link"
        ));
    }
    let before = (h.parent(oldest))
        .ok_or_else(|| format!("Commits {range} start the history: no comparison has them"))?;
    Ok(Link {
        url: web.compare(&before.to_string(), &newest.to_string(), true),
        what: format!("commits {range}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_core::git::{BranchRef, RefKind};
    use std::sync::Arc;

    #[test]
    fn a_comparison_since_the_merge_base_links_as_one() {
        let at = |name: &str, kind| BranchRef {
            name: name.to_owned(),
            refname: String::new(),
            kind,
            tip: Oid::ZERO_SHA1,
            is_head: false,
        };
        let h = History {
            branches: Arc::new(vec![
                at("main", RefKind::Local),
                at("origin/main", RefKind::Remote),
                at("origin/x", RefKind::Remote),
            ]),
            ..Default::default()
        };
        let web = WebRemote {
            base: "https://gitlab.example.com/acme/app".to_owned(),
            github: false,
        };
        let compare = |since| {
            let route = CompareRoute {
                base: "main".to_owned(),
                base_commit: String::new(),
                head: "origin/x".to_owned(),
                head_commit: String::new(),
                since,
            }
            .route();
            link(Some(&route), &web, false, &h).map(|l| (l.url, l.what))
        };
        assert_eq!(
            compare(true),
            Ok((
                "https://gitlab.example.com/acme/app/-/compare/main...x".to_owned(),
                "main...x".to_owned()
            ))
        );
        assert_eq!(
            compare(false),
            Ok((
                "https://gitlab.example.com/acme/app/-/compare/main...x?straight=true".to_owned(),
                "main..x".to_owned()
            ))
        );
    }
}
