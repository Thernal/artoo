//! What a request's pages are on the host (#162): the request's page — the one its host gave, else made from the
//! remote —, or its Changes, where the host anchors its files; a commit of it; a run of its commits as one diff; two
//! of its versions compared. The Inbox's own page is the request of the notice picked.

use artoo_branches_api::Branches;
use artoo_core::git::WebRemote;
use artoo_core::route::{Place, Route};
use artoo_requests_api::{
    InboxHost, InboxRow, RequestCommitRoute, RequestCommitsRoute, RequestRoute,
    RequestVersionsRoute, Requests,
};
use artoo_shell_api::{Link, Services};
use artoo_ui_kit::{format, words};
use git2::Oid;
use gpui::App;
use std::rc::Rc;

/// What a request's page is, in ⌘K's Copy link to…; the space's own page, the list, has none.
pub(crate) fn called(route: Option<&Route>) -> &'static str {
    match route.map(|r| r.kind.as_str()) {
        Some(RequestRoute::KIND) => "this merge request",
        Some(RequestCommitRoute::KIND) => "this commit",
        Some(RequestCommitsRoute::KIND) => "these commits",
        Some(RequestVersionsRoute::KIND) => "these versions",
        _ => "this page",
    }
}

/// A request's page `route` on host `web`; with `diff`, where the host shows its diff.
pub(crate) fn link(
    route: Option<&Route>,
    web: &WebRemote,
    diff: bool,
    services: &Services,
    cx: &App,
) -> Result<Link, String> {
    let nothing = || "Nothing here has a link: open a branch or a merge request".to_owned();
    let route = route.ok_or_else(nothing)?;
    let requests = services.get::<Rc<dyn Requests>>();
    let branches = services.get::<Rc<dyn Branches>>();
    let page = |number| request(web, number, requests.as_deref(), cx);
    let oid = |hex: &str| Oid::from_str(hex).map_err(|_| nothing());
    match route.kind.as_str() {
        RequestRoute::KIND => {
            let number = RequestRoute::from_route(route).ok_or_else(nothing)?.number;
            Ok(if diff {
                changes(web, page(number), None)
            } else {
                page(number)
            })
        }
        RequestCommitRoute::KIND => {
            let r = RequestCommitRoute::from_route(route).ok_or_else(nothing)?;
            Ok(changes(web, page(r.number), Some(oid(&r.commit)?)))
        }
        RequestCommitsRoute::KIND => {
            let r = RequestCommitsRoute::from_route(route).ok_or_else(nothing)?;
            let (newest, oldest) = (oid(&r.newest)?, oid(&r.oldest)?);
            run(web, page(r.number), newest, oldest, branches.as_deref(), cx)
        }
        RequestVersionsRoute::KIND => {
            let r = RequestVersionsRoute::from_route(route).ok_or_else(nothing)?;
            let version = |ix| branches.as_ref()?.version(ix, cx);
            let page = page(r.number);
            let (Some(a), Some(b)) = (version(r.from), version(r.to)) else {
                return Ok(page);
            };
            let id = |head| requests.as_ref()?.version_id(r.number, head, cx);
            Ok(versions(
                web,
                page,
                (a.number, a.tip),
                (b.number, b.tip),
                id,
            ))
        }
        _ => Err(nothing()),
    }
}

/// What the Inbox's notice picked is of: its request's page; why not when an answer of an agent is picked, or
/// nothing.
pub(crate) fn inbox_link(web: &WebRemote, services: &Services, cx: &App) -> Result<Link, String> {
    let requests = services.get::<Rc<dyn Requests>>();
    let answers = services
        .get::<Rc<dyn InboxHost>>()
        .map(|h| h.answers(cx))
        .unwrap_or_default();
    let number = requests
        .as_ref()
        .and_then(|r| match r.inbox_current(&answers, cx)? {
            InboxRow::Notice(ix) => r.notice_request(ix, cx),
            InboxRow::Agent(_) => None,
        });
    match number {
        Some(number) => Ok(request(web, number, requests.as_deref(), cx)),
        None => Err("Nothing here has a link: pick a notice of a merge request".to_owned()),
    }
}

/// Request `number`'s page: the one the host gave, else made from the remote.
fn request(web: &WebRemote, number: u64, requests: Option<&dyn Requests>, cx: &App) -> Link {
    Link {
        url: requests
            .and_then(|r| r.find(number, cx))
            .map_or_else(|| web.request(number), |m| m.url),
        what: words::number(number),
    }
}

/// A request's diff, `page` its page: GitLab's Changes, GitHub's Files, or one commit of it.
fn changes(web: &WebRemote, page: Link, commit: Option<Oid>) -> Link {
    let url = match (web.github, commit) {
        (true, None) => format!("{}/files", page.url),
        (true, Some(sha)) => format!("{}/commits/{sha}", page.url),
        (false, None) => format!("{}/diffs", page.url),
        (false, Some(sha)) => format!("{}/diffs?commit_id={sha}", page.url),
    };
    let what = match commit {
        Some(sha) => format!("{}, commit {}", page.what, format::short(sha)),
        None => page.what,
    };
    Link { url, what }
}

/// A request's commits from `oldest` to `newest` (a run picked with shift) as one diff on the host: GitHub's pull
/// request shows a run among its files; GitLab's has no such view, so its comparison of the commit before the run
/// with the newest shows the same diff.
fn run(
    web: &WebRemote,
    page: Link,
    newest: Oid,
    oldest: Oid,
    branches: Option<&dyn Branches>,
    cx: &App,
) -> Result<Link, String> {
    let range = format!("{}..{}", format::short(oldest), format::short(newest));
    if !branches.is_some_and(|b| b.pushed(newest, cx)) {
        return Err(format!(
            "Commits {range} are only on this Mac: push them for a link"
        ));
    }
    let before = branches
        .and_then(|b| b.parent(oldest, cx))
        .ok_or_else(|| format!("Commits {range} start the history: no comparison has them"))?;
    Ok(Link {
        url: if web.github {
            format!("{}/files/{before}..{newest}", page.url)
        } else {
            web.compare(&before.to_string(), &newest.to_string(), true)
        },
        what: format!("{}, commits {range}", page.what),
    })
}

/// Versions `a` and `b` of a request (each its number and head; `page` the request's) compared on the host: GitLab's
/// Compare versions (the newer named by its id, `id` of its head; the older by its head); GitHub's comparison of the
/// two heads, as its force-push events link it.
fn versions(
    web: &WebRemote,
    page: Link,
    a: (usize, Oid),
    b: (usize, Oid),
    id: impl Fn(Oid) -> Option<u64>,
) -> Link {
    let (a, b) = if a.0 <= b.0 { (a, b) } else { (b, a) };
    let what = format!("{}, v{} → v{}", page.what, a.0, b.0);
    if web.github {
        return Link {
            url: web.compare(&a.1.to_string(), &b.1.to_string(), true),
            what,
        };
    }
    let url = match id(b.1).filter(|id| *id > 0) {
        Some(id) if a.0 < b.0 => format!("{}/diffs?diff_id={id}&start_sha={}", page.url, a.1),
        Some(id) => format!("{}/diffs?diff_id={id}", page.url),
        None => format!("{}/diffs", page.url),
    };
    Link { url, what }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gitlab() -> WebRemote {
        WebRemote {
            base: "https://gitlab.example.com/acme/app".to_owned(),
            github: false,
        }
    }

    fn page() -> Link {
        Link {
            url: "https://gitlab.example.com/acme/app/-/merge_requests/398".to_owned(),
            what: "!398".to_owned(),
        }
    }

    #[test]
    fn a_requests_diff_is_its_changes_or_one_commit_of_it() {
        let sha = Oid::from_str("1e0acc5aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        assert_eq!(
            changes(&gitlab(), page(), None).url,
            format!("{}/diffs", page().url)
        );
        let commit = changes(&gitlab(), page(), Some(sha));
        assert_eq!(commit.url, format!("{}/diffs?commit_id={sha}", page().url));
        assert_eq!(commit.what, "!398, commit 1e0acc5");
    }

    #[test]
    fn two_versions_are_compared_by_the_newers_id_and_the_olders_head() {
        let (old, new) = (
            Oid::from_bytes(&[1; 20]).unwrap(),
            Oid::from_bytes(&[2; 20]).unwrap(),
        );
        let link = versions(&gitlab(), page(), (3, new), (1, old), |_| Some(77));
        assert_eq!(
            link.url,
            format!("{}/diffs?diff_id=77&start_sha={old}", page().url)
        );
        assert_eq!(link.what, "!398, v1 → v3");
        let unknown = versions(&gitlab(), page(), (1, old), (3, new), |_| None);
        assert_eq!(unknown.url, format!("{}/diffs", page().url));
    }
}
