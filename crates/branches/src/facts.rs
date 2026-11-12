//! What a branch's page says of it (#74), read when the branch loads: the main line it forks from, how far it is
//! from it, where it is pushed; and the line that says it, a piece over the branch's page (`line`).

use crate::history::{Act, Acts, History, NamedTip};
use artoo_branches_api::{BranchRoute, WorkingTreeRoute};
use artoo_core::git::Repo;
use artoo_core::route::{Place, Route};
use artoo_requests_api::Requests;
use artoo_shell_api::{Piece, Services, Slot};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::{island, px, theme, words};
use git2::Oid;
use gpui::{AnyElement, App, Div, Entity, MouseButton, MouseDownEvent, div, prelude::*};
use std::rc::Rc;

/// What a branch's page says of it, read when the branch loads.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// The branch it is a page of.
    pub refname: String,
    /// The main line it forks from, at its tip; none for the main line itself.
    pub base: Option<NamedTip>,
    /// Commits it has that the main line has not, and the other way round.
    pub ahead: usize,
    pub behind: usize,
    /// Its upstream, with how far it is ahead of it and behind it; none when it was never pushed.
    pub upstream: Option<(String, usize, usize)>,
    /// The files of its Changes, once read.
    pub files: Option<usize>,
}

impl Facts {
    /// Reads them for branch `name` (`refname`) at `tip`: its base is the main line's local branch,
    /// else its remote one.
    pub fn read(repo: &Repo, refname: &str, name: &str, tip: Oid) -> Facts {
        let main = repo.default_branch();
        let base = main
            .as_deref()
            .filter(|m| name != *m && name != format!("origin/{m}"))
            .and_then(|m| {
                [m.to_owned(), format!("origin/{m}")]
                    .into_iter()
                    .find_map(|n| repo.resolve(&n).ok().map(|id| (n, id)))
            });
        let (ahead, behind) = base
            .as_ref()
            .and_then(|(_, b)| repo.ahead_behind(tip, *b))
            .unwrap_or_default();
        Facts {
            refname: refname.to_owned(),
            base,
            ahead,
            behind,
            upstream: repo.tracking(name),
            files: None,
        }
    }
}

/// A branch's line over its page with tabs (`Slot::Identity`): its request's number the requests' (`Requests`), a
/// click on it the tab's answer to the history's acts (`Acts`), both from the tab's `services`.
pub fn line(history: Entity<History>, services: Rc<Services>) -> Piece {
    Piece {
        id: "branches.line",
        slot: Slot::Identity,
        draw: Rc::new(move |route, _, cx| line_of(&history, &services, route, cx)),
    }
}

/// The line of the branch `route` is, while it is the one with tabs loaded.
pub fn line_of(
    history: &Entity<History>,
    services: &Services,
    route: &Route,
    cx: &App,
) -> Option<AnyElement> {
    let acts = services.get::<Acts>()?;
    let h = history.read(cx);
    // The Working tree of the branch checked out says which it is first (#209).
    let checked_out = h
        .checked_out()
        .filter(|_| h.selection == crate::history::Selection::WorkingTree);
    let refname = match BranchRoute::from_route(route) {
        Some(r) => r.refname,
        None if WorkingTreeRoute::from_route(route).is_some() && checked_out.is_some() => {
            h.branches.get(h.branch?)?.refname.clone()
        }
        None => return None,
    };
    if !h.tabbed(&refname) {
        return None;
    }
    let request = h
        .branch
        .and_then(|ix| h.branches.get(ix))
        .and_then(|b| services.get::<Rc<dyn Requests>>()?.of_branch(b, cx))
        .map(|(iid, _)| iid);
    let checked_out = checked_out.map(|s| s.branch.0);
    let line = branch_line(h.page.as_ref()?, h.branch, request, checked_out, &acts)?;
    Some(line.into_any_element())
}

/// One line over a branch's Changes and Commits: ahead of the main line, behind it, where it is pushed, its
/// `request`'s number; over the Working tree first the branch `checked_out` (#209); a right click on it branch
/// `ix`'s menu. None for the main line itself.
pub fn branch_line(
    f: &Facts,
    ix: Option<usize>,
    request: Option<u64>,
    checked_out: Option<String>,
    acts: &Acts,
) -> Option<Div> {
    let main = f.base.as_ref()?.0.trim_start_matches("origin/").to_owned();
    let request = request.map_or_else(|| "none yet".to_owned(), words::number);
    let acts = acts.clone();
    Some(
        facts_row()
            // Its link, the branch's name, a request (#162).
            .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, _, cx| {
                if let Some(ix) = ix {
                    acts(Act::BranchMenu { ix, at: e.position }, cx)
                }
            })
            .children(checked_out.map(|name| mono_fact("Checked out".to_owned(), name)))
            .child(fact(format!("Ahead of {main}"), plural(f.ahead, "commit")))
            .child(fact("Behind".to_owned(), f.behind.to_string()))
            .child(fact("Remote".to_owned(), remote(f.upstream.as_ref())))
            .child(fact(words::say("Request").into_owned(), request)),
    )
}

/// The main line's own row over its page (#208): where it is pushed and its latest commit, `231a547 · 25 d ago`.
/// None until its facts and commits are read.
pub fn main_line(h: &History) -> Option<AnyElement> {
    let f = h.page.as_ref()?;
    let latest = h.commits.first()?;
    Some(
        facts_row()
            .child(fact("Remote".to_owned(), remote(f.upstream.as_ref())))
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(div().text_color(theme::faint()).child("Latest"))
                    .child(
                        div()
                            .font_family(theme::code_font())
                            .child(format::short(latest.id)),
                    )
                    .child(div().text_color(theme::faint()).child("·"))
                    .child(div().child(format::ago(latest.time))),
            )
            .into_any_element(),
    )
}

/// Where a branch is pushed, `upstream` its upstream with how far it is ahead of it and behind it: `origin · in
/// sync`, `2 commits not pushed`, `3 behind origin/main`, `not pushed`.
fn remote(upstream: Option<&(String, usize, usize)>) -> String {
    match upstream {
        None => "not pushed".to_owned(),
        Some((up, 0, 0)) => format!("{} · in sync", up.split('/').next().unwrap_or(up)),
        Some((_, ahead, 0)) => format!("{} not pushed", plural(*ahead, "commit")),
        Some((up, _, behind)) => format!("{behind} behind {up}"),
    }
}

/// A row of facts, one line over the page.
fn facts_row() -> Div {
    island()
        .flex_none()
        .flex_row()
        .items_center()
        .gap(px(20.))
        .h(px(40.))
        .px(px(14.))
        .text_size(px(12.))
}

/// A fact whose value is a name in code's font: a branch.
fn mono_fact(label: String, value: String) -> Div {
    div()
        .flex()
        .gap(px(6.))
        .child(div().text_color(theme::faint()).child(label))
        .child(div().font_family(theme::code_font()).child(value))
}

/// A fact, its label faint.
fn fact(label: String, value: String) -> Div {
    div()
        .flex()
        .gap(px(6.))
        .child(div().text_color(theme::faint()).child(label))
        .child(div().child(value))
}
