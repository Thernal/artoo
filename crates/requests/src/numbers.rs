//! A request's number in the tab's texts (#171, D11): `!398` on GitLab, `#12` on GitHub, a link to the request on its
//! host, which the app opens itself (the host kinds read it back). On GitHub `#12` is an issue's number as well: one
//! of the requests read here goes to its pull request, any other to the issues, where GitHub sends a pull request on.

use crate::requests::Requests;
use artoo_core::git::WebRemote;
use artoo_core::links;
use artoo_shell_api::links::Pattern;
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

/// The requests' numbers as links, for the tab whose requests these are.
pub(crate) fn pattern(requests: &Entity<Requests>, cx: &mut App) -> Pattern {
    let known = Rc::new(RefCell::new(Known::default()));
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
