//! Branches' pages (#72): their own, the list (`list`) in the tab's frame of a list beside its preview
//! (`Frame::list_page`), the row picked previewed; and a branch's, a comparison's and the working tree's — a branch's
//! Commits (`commits_page`), else the review's item page (`artoo_review_api::ItemView`), the side panel beside
//! (`panel`). What other features know of a branch comes from the tab's services — your review of its tip (`Review`),
//! its request (`Requests`) —; a click asks the tab an `Act`. What a route shows is read as it comes on screen
//! (`Branches::show`).

use crate::card::branch_card;
use crate::commits_page::BranchCommits;
use crate::history::{Acts, History};
use crate::list::{self, Beside, Card, ListLook};
use crate::panel::Panel;
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, Branches, CompareRoute, VersionsRoute,
    WorkingTreeRoute,
};
use artoo_core::route::{Place, Route};
use artoo_requests_api::Requests;
use artoo_review_api::{ItemView, Review};
use artoo_shell_api::{
    Drawing, Frame, ItemRow, NameHover, PagePath, Pages, PathUp, Screen, Services,
};
use artoo_ui_kit::format::plural;
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{px, theme};
use gpui::{AnyElement, App, Context, Entity, Render, Window, div, prelude::*};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

/// Branches' own page in one tab.
pub struct RootPage {
    history: Entity<History>,
    frame: Rc<dyn Frame>,
    /// The list's width as last drawn: the room its rows have (#79).
    widths: Widths,
}

/// Branches' pages in a tab: their own page, the list; a branch's, a comparison's, the working tree's, `panel`
/// beside them.
pub fn pages(
    history: Entity<History>,
    panel: Entity<Panel>,
    frame: Rc<dyn Frame>,
    cx: &mut App,
) -> Pages {
    let services = Rc::downgrade(&frame.services());
    let (listed, linked) = (history.downgrade(), history.downgrade());
    let view = cx.new(|cx| {
        cx.observe(&history, |_, _, cx| cx.notify()).detach();
        RootPage {
            history,
            frame,
            widths: Widths::default(),
        }
    });
    Pages {
        space: artoo_branches_api::SPACE,
        kinds: &[
            BranchRoute::KIND,
            BranchCommitRoute::KIND,
            BranchCommitsRoute::KIND,
            VersionsRoute::KIND,
            WorkingTreeRoute::KIND,
            CompareRoute::KIND,
        ],
        parent,
        item,
        path: Rc::new({
            let services = services.clone();
            move |route, drawing, cx| {
                let services = services.upgrade()?;
                path(route, drawing, &services, cx)
            }
        }),
        screen,
        row: Rc::new({
            let services = services.clone();
            move |item, page, cx| {
                let services = services.upgrade()?;
                row(item, page, &services, cx)
            }
        }),
        called: crate::link::called,
        link: Rc::new(move |route, web, diff, cx| {
            let history = linked
                .upgrade()
                .ok_or_else(|| "Nothing here has a link".to_owned())?;
            crate::link::link(route, web, diff, history.read(cx))
        }),
        view: view.into(),
        side: Some(Rc::new(move |route, width, cx| {
            route?;
            panel.update(cx, |p, _| p.width = width);
            Some(panel.clone().into_any_element())
        })),
        show: Rc::new(move |route, _, cx| {
            let Some(services) = services.upgrade() else {
                return;
            };
            match (route, services.get::<Rc<dyn Branches>>()) {
                (Some(route), Some(branches)) => branches.show(route, cx),
                (Some(_), None) => {}
                // The list: what nothing picked picks, what its rows say read.
                (None, _) => {
                    let settings = services
                        .get::<Rc<dyn artoo_review_api::Review>>()
                        .map(|r| r.settings(cx))
                        .unwrap_or_default();
                    let Some(history) = listed.upgrade() else {
                        return;
                    };
                    history.update(cx, |h, cx| {
                        h.roots.picked = h.roots.picked.or(h.first_row());
                        h.read_facts(cx);
                        h.read_tree(settings, cx);
                    });
                }
            }
        }),
        keys: None,
    }
}

impl Render for RootPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.frame.route(cx) {
            Some(route) => self.item(route, cx),
            None => self.list(cx),
        }
    }
}

impl RootPage {
    /// A branch's Commits, else the review's item page.
    fn item(&self, route: Route, cx: &mut Context<Self>) -> AnyElement {
        let services = self.frame.services();
        if self.frame.item_tab(cx).is_some()
            && let Some(commits) = services.get::<Entity<BranchCommits>>()
        {
            commits.update(cx, |c, _| c.route = Some(route));
            return commits.into_any_element();
        }
        item_page(&services)
    }

    /// The list beside its preview.
    fn list(&self, cx: &mut Context<Self>) -> AnyElement {
        let services = self.frame.services();
        let filter = self.frame.filter(cx);
        let acts = services.get::<Acts>().unwrap_or_else(|| Rc::new(|_, _| {}));
        let (rows, preview) = {
            let h = self.history.read(cx);
            let rows = h.rows(&filter);
            let picked = h
                .roots
                .picked
                .filter(|r| rows.contains(r))
                .or_else(|| h.first_row().filter(|r| rows.contains(r)))
                .or(rows.first().copied());
            let beside = |ix: usize| beside(h, ix, &services, cx);
            let look = ListLook {
                room: self.widths.room("branches"),
                changes: h.wt.count,
                filter: filter.trim(),
                picked,
                beside: &beside,
            };
            (
                list::rows(h, &rows, &look, &acts),
                list::preview(h, &look, &acts),
            )
        };
        let list = div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.widths.measure("branches"))
            .child(
                div()
                    .id("branches")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .pb(px(6.))
                    .children(rows),
            )
            .into_any_element();
        let preview = div()
            .id("branch-preview")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(preview)
            .into_any_element();
        // A column: the page is stretched across to its width, whatever its content's least width.
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.frame.list_page(list, preview, cx))
            .into_any_element()
    }
}

/// The review's item page (`ItemView`) in a column the page's width.
fn item_page(services: &Services) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .children(services.get::<ItemView>().map(|v| v.0))
        .into_any_element()
}

/// What the list shows of branch `ix` that other features know: your review of its tip, its request, its card.
fn beside(h: &History, ix: usize, services: &Services, cx: &App) -> Beside {
    let Some(b) = h.branches.get(ix) else {
        return Beside::default();
    };
    let review = services.get::<Rc<dyn Review>>();
    let request = services
        .get::<Rc<dyn Requests>>()
        .and_then(|r| r.of_branch(b, cx));
    Beside {
        part: review.as_ref().and_then(|r| r.head_part(b.tip, cx)),
        reviewed: review.is_some_and(|r| r.head_reviewed(b.tip, cx)),
        card: branch_card(h, ix, request.clone()).map(|card| Box::new(card) as Card),
        request,
    }
}

/// The menu's row of what is open under Branches: a branch by its name, the working tree, two refs compared named as
/// their page is (#149).
fn row(item: &Route, page: &Route, services: &Services, cx: &App) -> Option<ItemRow> {
    let (label, icon, mono) = match item.kind.as_str() {
        BranchRoute::KIND => {
            let refname = BranchRoute::from_route(item)?.refname;
            let list = (services.get::<Rc<dyn Branches>>())
                .map(|b| b.list(cx))
                .unwrap_or_default();
            let name =
                (list.iter().find(|b| b.refname == refname)).map_or(refname, |b| b.name.clone());
            (name, "branch", true)
        }
        WorkingTreeRoute::KIND => ("Working tree".to_owned(), "files", false),
        ComparisonRoute::KIND => {
            let label = CompareRoute::from_route(page)
                .map_or_else(|| page.kind.clone(), |c| format!("{} … {}", c.base, c.head));
            (label, "layers", true)
        }
        _ => return None,
    };
    Some(ItemRow {
        icon: row_icon(icon),
        tip: label.clone(),
        label,
        mono,
        news: false,
    })
}

/// A row's icon in the menu, as the menu draws its rows'.
fn row_icon(name: &'static str) -> AnyElement {
    artoo_ui_kit::icons::icon(name)
        .size(px(14.))
        .text_color(theme::faint())
        .into_any_element()
}

/// The space's own page is its list; a route of theirs, an item's screen.
fn screen(route: Option<&Route>) -> Screen {
    if route.is_some() {
        Screen::Item
    } else {
        Screen::List
    }
}

/// The bar's path to a page of Branches: Branches, then the branch a commit, a run or two versions are of (its
/// page), Commits over a commit or a run (the branch's page, where they are picked); a commit's card or the branch's
/// in the name's long hover.
fn path(route: &Route, drawing: &Drawing, services: &Services, cx: &App) -> Option<PagePath> {
    let branches = services.get::<Rc<dyn Branches>>()?;
    let list = branches.list(cx);
    let at = |refname: &str| list.iter().position(|b| b.refname == refname);
    let named =
        |refname: &str| at(refname).map_or_else(|| refname.to_owned(), |ix| list[ix].name.clone());
    let versions = |a: usize, b: usize| match (branches.version(a, cx), branches.version(b, cx)) {
        (Some(a), Some(b)) => format!("v{} → v{}", a.number, b.number),
        _ => "versions".to_owned(),
    };
    // The branch's card names the request riding on it.
    let card = |refname: &str| {
        let ix = at(refname)?;
        let requests = services.get::<Rc<dyn Requests>>();
        let request = requests.as_ref().and_then(|r| {
            let (number, _) = r.of_branch(&list[ix], cx)?;
            r.find(number, cx).map(|m| (m.iid, m.title))
        });
        branches.branch_card(ix, request, cx).map(NameHover::Card)
    };
    let commit = |id: git2::Oid| {
        branches
            .commit_card(id, drawing.web.as_ref(), cx)
            .map(NameHover::Card)
    };
    let oid = |hex: &str| git2::Oid::from_str(hex).ok();
    let root = || PathUp::new("Branches", None);
    let branch = |refname: &str| {
        let page = BranchRoute {
            refname: refname.to_owned(),
        }
        .route();
        let mut name = PathUp::new(named(refname), Some(page.clone()));
        name.mono = true;
        (name, PathUp::new("Commits", Some(page)))
    };
    let short = artoo_ui_kit::format::short;
    // A run says how many commits it is (#208), its ends while they are not among the commits read.
    let run = |newest: &str, oldest: &str| {
        let (newest, oldest) = (oid(newest)?, oid(oldest)?);
        Some(
            match (
                branches.commit_of(newest, cx),
                branches.commit_of(oldest, cx),
            ) {
                (Some(a), Some(b)) if b >= a => (plural(b - a + 1, "commit"), false),
                _ => (format!("{}..{}", short(oldest), short(newest)), true),
            },
        )
    };
    let (up, name, mono, hover) = match route.kind.as_str() {
        BranchRoute::KIND => {
            let r = BranchRoute::from_route(route)?;
            (vec![root()], named(&r.refname), true, card(&r.refname))
        }
        // A commit is a level under the branch's Commits (#74).
        BranchCommitRoute::KIND => {
            let r = BranchCommitRoute::from_route(route)?;
            let (name, commits) = branch(&r.refname);
            let id = oid(&r.commit)?;
            (vec![root(), name, commits], short(id), true, commit(id))
        }
        BranchCommitsRoute::KIND => {
            let r = BranchCommitsRoute::from_route(route)?;
            let (name, commits) = branch(&r.refname);
            let (id, mono) = run(&r.newest, &r.oldest)?;
            (vec![root(), name, commits], id, mono, card(&r.refname))
        }
        VersionsRoute::KIND => {
            let r = VersionsRoute::from_route(route)?;
            let (name, _) = branch(&r.refname);
            (
                vec![root(), name],
                versions(r.from, r.to),
                false,
                card(&r.refname),
            )
        }
        WorkingTreeRoute::KIND => (vec![root()], "Working tree".to_owned(), false, None),
        CompareRoute::KIND => {
            let r = CompareRoute::from_route(route)?;
            (vec![root()], format!("{} … {}", r.base, r.head), true, None)
        }
        _ => return None,
    };
    Some(PagePath {
        up,
        name,
        mono,
        hover,
        count: None,
        title: None,
    })
}

/// Two refs compared as what stays open under Branches (#149): by their names, whichever is the base, so Swap and
/// Direct keep its row. No page draws it; its page is the comparison it was left on.
#[derive(Serialize, Deserialize)]
struct ComparisonRoute {
    a: String,
    b: String,
}

impl Place for ComparisonRoute {
    const KIND: &'static str = "branches.comparison";
}

/// The branch a commit, a run or two versions of it are under; a branch, a comparison and the working tree are
/// under Branches' own page.
fn parent(route: &Route) -> Option<Route> {
    let refname = match route.kind.as_str() {
        BranchCommitRoute::KIND => BranchCommitRoute::from_route(route)?.refname,
        BranchCommitsRoute::KIND => BranchCommitsRoute::from_route(route)?.refname,
        VersionsRoute::KIND => VersionsRoute::from_route(route)?.refname,
        _ => return None,
    };
    Some(BranchRoute { refname }.route())
}

/// What stays open under Branches: the branch a page is of, the working tree, a comparison by its refs' names.
fn item(route: &Route) -> Option<Route> {
    let refname = match route.kind.as_str() {
        BranchRoute::KIND => BranchRoute::from_route(route)?.refname,
        BranchCommitRoute::KIND => BranchCommitRoute::from_route(route)?.refname,
        BranchCommitsRoute::KIND => BranchCommitsRoute::from_route(route)?.refname,
        VersionsRoute::KIND => VersionsRoute::from_route(route)?.refname,
        WorkingTreeRoute::KIND => return Some(WorkingTreeRoute {}.route()),
        CompareRoute::KIND => {
            let compare = CompareRoute::from_route(route)?;
            let (a, b) = if compare.base <= compare.head {
                (compare.base, compare.head)
            } else {
                (compare.head, compare.base)
            };
            return Some(ComparisonRoute { a, b }.route());
        }
        _ => return None,
    };
    Some(BranchRoute { refname }.route())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branchs_pages_are_under_it_and_keep_it_open() {
        let x = || "refs/heads/x".to_owned();
        let branch = BranchRoute { refname: x() }.route();
        let commit = BranchCommitRoute {
            refname: x(),
            commit: "0123".to_owned(),
        }
        .route();
        let versions = VersionsRoute {
            refname: x(),
            from: 1,
            to: 2,
        }
        .route();
        for page in [&commit, &versions] {
            assert_eq!(parent(page), Some(branch.clone()));
            assert_eq!(item(page), Some(branch.clone()));
        }
        assert_eq!(parent(&branch), None);
        assert_eq!(item(&branch), Some(branch.clone()));
        let tree = WorkingTreeRoute {}.route();
        assert_eq!((parent(&tree), item(&tree)), (None, Some(tree.clone())));
    }

    /// A comparison stays one row whichever side is the base, and since its merge base or not.
    #[test]
    fn a_comparison_is_kept_open_by_its_refs_either_way_round() {
        let compare = |base: &str, head: &str, since| {
            CompareRoute {
                base: base.to_owned(),
                base_commit: "a".to_owned(),
                head: head.to_owned(),
                head_commit: "b".to_owned(),
                since,
            }
            .route()
        };
        let one = item(&compare("main", "origin/x", true));
        assert_eq!(one, item(&compare("origin/x", "main", false)));
        assert_eq!(one.map(|r| r.kind), Some(ComparisonRoute::KIND.to_owned()));
        assert_eq!(parent(&compare("main", "origin/x", true)), None);
    }
}
