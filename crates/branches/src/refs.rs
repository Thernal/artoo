//! A comparison's two picks (Compare two refs…, ⌥⌘C; `artoo_shell_api::search::List`): the base — a branch, a tag or
//! a commit —, then what to compare with it, each list the repository's refs ranked by the text, or anything git
//! names. The second pick opens the comparison's page, since their merge base; esc there goes back to the first.

use crate::history::History;
use artoo_branches_api::{COMPARE, COMPARE_WITH, CompareRoute};
use artoo_core::git::{NamedRef, Repo};
use artoo_core::rank::rank;
use artoo_core::route::Place;
use artoo_shell_api::Frame;
use artoo_shell_api::search::{Find, List, Opens, Said};
use artoo_ui_kit::format;
use git2::Oid;
use gpui::WeakEntity;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// What the two lists share: the repository and its refs, read as the first opens, and the base picked there.
#[derive(Default)]
struct Picking {
    root: Option<PathBuf>,
    refs: Vec<NamedRef>,
    base: Option<(String, Oid)>,
}

/// The two lists, reading the repository the history is of.
pub fn lists(history: WeakEntity<History>, frame: Rc<dyn Frame>) -> Vec<List> {
    let picking: Rc<RefCell<Picking>> = Rc::default();
    let (read, bases, withs, named) = (
        picking.clone(),
        picking.clone(),
        picking.clone(),
        picking.clone(),
    );
    let base = List {
        id: COMPARE,
        says: Rc::new(|_| Said {
            title: Some("Compare · step 1 of 2".to_owned()),
            placeholder: "Compare — pick the base: a branch, a tag or a commit",
            empty: "No branch, tag or commit by that name.",
        }),
        find: Rc::new(move |ask, _| {
            let next = |name: String, id: Oid| {
                let (picking, frame) = (picking.clone(), frame.clone());
                Opens::Run(Rc::new(move |cx| {
                    picking.borrow_mut().base = Some((name.clone(), id));
                    frame.search(COMPARE_WITH, cx);
                }))
            };
            finds(ask.text, &bases.borrow(), "Base", next)
        }),
        opened: Some(Rc::new(move |cx| {
            let root = history.upgrade().and_then(|h| h.read(cx).root.clone());
            let refs = (root.as_ref())
                .and_then(|root| Repo::open(root).and_then(|r| r.compare_refs()).ok())
                .unwrap_or_default();
            *read.borrow_mut() = Picking {
                root,
                refs,
                base: None,
            };
        })),
        closed: None,
        back: None,
        tab: None,
    };
    let with = List {
        id: COMPARE_WITH,
        says: Rc::new(move |_| {
            let name = (named.borrow().base.as_ref()).map_or(String::new(), |(n, _)| n.clone());
            Said {
                title: Some(format!("Compare · {name} … ?")),
                placeholder: "Compare — pick what to compare with it",
                empty: "No branch, tag or commit by that name.",
            }
        }),
        find: Rc::new(move |ask, _| {
            let picking = withs.borrow();
            let Some((base, base_id)) = picking.base.clone() else {
                return Vec::new();
            };
            let page = |head: String, id: Oid| {
                let route = CompareRoute {
                    base: base.clone(),
                    base_commit: base_id.to_string(),
                    head,
                    head_commit: id.to_string(),
                    since: true,
                };
                Opens::Route(route.route())
            };
            finds(ask.text, &picking, "Compare with", page)
        }),
        opened: None,
        closed: None,
        back: Some(COMPARE),
        tab: None,
    };
    vec![base, with]
}

/// The refs ranked by `query` — a stale local branch after its upstream, the one meant —, 14 of them; first, what
/// git names by the text when no ref is called that (a commit id, `HEAD~3`, a ref the list does not show).
fn finds(
    query: &str,
    picking: &Picking,
    group: &'static str,
    pick: impl Fn(String, Oid) -> Opens,
) -> Vec<Find> {
    let mut ranked = rank(&picking.refs, query, |r| &r.name);
    for i in 0..ranked.len() {
        if let Some(st) = &ranked[i].stale
            && let Some(j) = ranked[i..].iter().position(|r| r.name == st.upstream)
        {
            let upstream = ranked.remove(i + j);
            ranked.insert(i, upstream);
        }
    }
    let mut finds: Vec<Find> = (ranked.into_iter().take(14))
        .map(|r| {
            let detail = match &r.stale {
                Some(st) => format!(
                    "branch {} · ↓{} behind {}",
                    format::short(r.tip),
                    st.behind,
                    st.upstream
                ),
                None => format!(
                    "{} {}",
                    if r.tag { "tag" } else { "branch" },
                    format::short(r.tip)
                ),
            };
            Find::new(group, r.name.clone(), detail, pick(r.name.clone(), r.tip))
        })
        .collect();
    if !query.is_empty()
        && !picking.refs.iter().any(|r| r.name == query.trim())
        && let Some(id) =
            (picking.root.as_ref()).and_then(|root| Repo::open(root).ok()?.resolve(query).ok())
    {
        let name = query.trim().to_owned();
        let detail = format!("commit {}", format::short(id));
        finds.insert(0, Find::new(group, name.clone(), detail, pick(name, id)));
    }
    finds
}
