//! A request's number in the tab's texts (#171, D11): `!398` on GitLab, `#12` on GitHub, a link to the request on its
//! host, which the app opens itself (the host kinds read it back). On GitHub `#12` is an issue's number as well: one
//! of the requests read here goes to its pull request, any other to the issues, where GitHub sends a pull request on.

use crate::requests::Requests;
use artoo_core::git::WebRemote;
use artoo_core::links;
use artoo_shell_api::links::{Claim, Kind, Pattern};
use artoo_ui_kit::words;
use gpui::{App, Entity};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// What the pattern reads, kept as the requests change: their remote and the numbers read. Kept apart from the
/// requests so finding links never reads an entity that may be drawing.
#[derive(Default)]
struct Known {
    web: Option<WebRemote>,
    read: HashSet<u64>,
}

/// The requests' links for the tab whose requests these are: their numbers found in text, and what a request's
/// link is (its menu, `kind`).
pub(crate) fn links(requests: &Entity<Requests>, cx: &mut App) -> (Pattern, Rc<dyn Kind>) {
    let known = Rc::new(RefCell::new(Known::default()));
    (pattern(requests, known.clone(), cx), Rc::new(Link(known)))
}

/// A link to a request of the open project, by its page, a comment of it or a line of its diff.
struct Link(Rc<RefCell<Known>>);

impl Kind for Link {
    fn id(&self) -> &'static str {
        "requests.link"
    }

    fn claim(&self, address: &str, cx: &App) -> Option<Claim> {
        let link = artoo_hosts_api::get(cx).link(address)?;
        claim_of(self.0.borrow().web.as_ref()?, &link, address)
    }
}

/// What `link`, written `address`, is, when it is a request of project `web`.
fn claim_of(web: &WebRemote, link: &links::Link, address: &str) -> Option<Claim> {
    let links::Target::Request { iid, .. } = link.target else {
        return None;
    };
    (link.base == web.base).then(|| Claim {
        name: words::number(iid),
        icon: "pull-request",
        url: Some(address.to_owned()),
        site: Some(web.name().to_owned()),
        copy_name: true,
    })
}

/// The requests' numbers as links.
fn pattern(requests: &Entity<Requests>, known: Rc<RefCell<Known>>, cx: &mut App) -> Pattern {
    let keep = known.clone();
    let update = move |requests: &Entity<Requests>, cx: &App| {
        let r = requests.read(cx);
        *keep.borrow_mut() = Known {
            web: r.web.clone(),
            read: r
                .list
                .iter()
                .chain(r.past.scopes.iter().flat_map(|p| &p.list))
                .chain(&r.left)
                .map(|m| m.iid)
                .collect(),
        };
    };
    update(requests, cx);
    cx.observe(requests, move |requests, cx| update(&requests, cx))
        .detach();
    Pattern {
        id: "requests.number",
        order: 10,
        find: Rc::new(move |text, _| {
            let known = known.borrow();
            let Some(web) = known.web.as_ref() else {
                return Vec::new();
            };
            let sign = if web.github { '#' } else { '!' };
            links::numbers(text, sign)
                .into_iter()
                .map(|(range, iid)| {
                    let url = if web.github && !known.read.contains(&iid) {
                        format!("{}/issues/{iid}", web.base)
                    } else {
                        web.request(iid)
                    };
                    (range, url)
                })
                .collect()
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_requests_link_is_claimed_for_its_own_project_alone() {
        let web = WebRemote {
            base: "https://gitlab.example.com/acme/app".to_owned(),
            github: false,
        };
        let claim = |url: &str| claim_of(&web, &links::parse(url)?, url);
        let url = "https://gitlab.example.com/acme/app/-/merge_requests/412#note_7";
        let found = claim(url).unwrap();
        assert_eq!(found.url.as_deref(), Some(url));
        assert_eq!(found.open_in(), "Open in GitLab");
        assert!(claim("https://gitlab.example.com/acme/other/-/merge_requests/412").is_none());
        assert!(claim("https://gitlab.example.com/acme/app/-/commit/1e0acc5").is_none());
    }
}
