//! What a branch's page says of it (#74), read when the branch loads: the main line it forks from, how far it is
//! from it, where it is pushed; and the line that says it, a piece over the branch's page (`line`).

use crate::history::{Act, Acts, History, NamedTip};
use artoo_branches_api::{BranchRoute, WorkingTreeRoute};
use artoo_core::git::Repo;
use artoo_core::route::{Place, Route};
use artoo_shell_api::{Piece, Services, Slot};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::Hover;
use artoo_ui_kit::{icons, island, px, theme};
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
        draw: Rc::new(move |route, _, cx| {
            // A new merge request says them at the end of its Request · Changes (`Branches::facts`): no line of its
            // own over the page (the owner, 2026-11-26).
            if (services.get::<Rc<dyn artoo_requests_api::Requests>>())
                .is_some_and(|r| r.creating(cx))
            {
                return None;
            }
            line_of(&history, &services, route, cx)
        }),
    }
}

/// The branch's facts in a quiet run for another part's row: how far it is from the main line and where it is
/// pushed, each its sentence's tooltip. None for the main line.
pub fn inline(f: &Facts) -> Option<Div> {
    let main = f.base.as_ref()?.0.trim_start_matches("origin/").to_owned();
    Some(
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(14.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(
                div()
                    .id("fact-ahead")
                    .flex()
                    .gap(px(8.))
                    .child(format!("↑{}", f.ahead))
                    .child(format!("↓{}", f.behind))
                    .tip(format!(
                        "{} ahead of {main}, {} behind",
                        plural(f.ahead, "commit"),
                        f.behind
                    )),
            )
            .child(origin_fact(f.upstream.as_ref())),
    )
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
    let checked_out = checked_out.map(|s| s.branch.0);
    let line = branch_line(h.page.as_ref()?, h.branch, checked_out, &acts)?;
    Some(line.into_any_element())
}

/// One line over a branch's Changes and Commits: `↑2 ↓0` how far it is from the main line, where it is pushed
/// (a glyph and the remote's name); its request is the bar's chip; over the Working tree first the branch `checked_out`
/// (#209) last; a right click on it branch `ix`'s menu. None for the main line itself. Each fact says its
/// sentence in its tooltip.
pub fn branch_line(
    f: &Facts,
    ix: Option<usize>,
    checked_out: Option<String>,
    acts: &Acts,
) -> Option<Div> {
    let main = f.base.as_ref()?.0.trim_start_matches("origin/").to_owned();
    let acts = acts.clone();
    let ahead = format!(
        "{} ahead of {main}, {} behind",
        plural(f.ahead, "commit"),
        f.behind
    );
    Some(
        facts_row()
            // Its link, the branch's name, a request (#162).
            .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, _, cx| {
                if let Some(ix) = ix {
                    acts(Act::BranchMenu { ix, at: e.position }, cx)
                }
            })
            .child(
                div()
                    .id("fact-ahead")
                    .flex()
                    .gap(px(8.))
                    .child(format!("↑{}", f.ahead))
                    .child(format!("↓{}", f.behind))
                    .tip(ahead),
            )
            .child(origin_fact(f.upstream.as_ref()))
            .children(checked_out.map(|name| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .child(
                        icons::icon("branch")
                            .size(px(14.))
                            .text_color(theme::faint()),
                    )
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .font_family(theme::code_font())
                            .child(crate::key::name("fact-checked-out", name)),
                    )
            })),
    )
}

/// The main line's own row over its page (#208): where it is pushed and its latest commit, `231a547 25d`, its
/// sentence the tooltip. None until its facts and commits are read.
pub fn main_line(h: &History) -> Option<AnyElement> {
    let f = h.page.as_ref()?;
    let latest = h.commits.first()?;
    let id = format::short(latest.id);
    Some(
        facts_row()
            .child(origin_fact(f.upstream.as_ref()))
            .child(
                div()
                    .id("fact-latest")
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        icons::icon("git-commit")
                            .size(px(14.))
                            .text_color(theme::faint()),
                    )
                    .child(div().font_family(theme::code_font()).child(id.clone()))
                    .child(div().child(format::short_ago(latest.time)))
                    .tip_two(
                        "Latest commit on main",
                        format!("{id}, {}", format::ago(latest.time)),
                    ),
            )
            .into_any_element(),
    )
}

/// Where a branch is pushed, `upstream` its upstream with how far it is ahead of it and behind it: the cloud and
/// the remote's name (`origin`), `↑2` after it while commits are not pushed, `↓3` while it is behind; `not pushed`
/// without one. The sentence is its tooltip.
fn origin_fact(upstream: Option<&(String, usize, usize)>) -> gpui::Stateful<Div> {
    let (word, tip) = match upstream {
        None => (
            "not pushed".to_owned(),
            "This branch was never pushed".to_owned(),
        ),
        Some((up, 0, 0)) => (
            up.split('/').next().unwrap_or(up).to_owned(),
            format!("In sync with {up}"),
        ),
        Some((up, ahead, 0)) => (
            format!("{} ↑{ahead}", up.split('/').next().unwrap_or(up)),
            format!("{} not pushed to {up}", plural(*ahead, "commit")),
        ),
        Some((up, ahead, behind)) => (
            format!("{} ↑{ahead} ↓{behind}", up.split('/').next().unwrap_or(up)),
            format!("{ahead} ahead of {up}, {behind} behind"),
        ),
    };
    div()
        .id("fact-origin")
        .flex()
        .items_center()
        .gap(px(6.))
        .child(
            icons::icon("cloud")
                .size(px(14.))
                .text_color(theme::faint()),
        )
        .child(div().child(word))
        .tip(tip)
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
