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
use artoo_shell_api::{Frame, Pages, Services};
use artoo_ui_kit::px;
use artoo_ui_kit::room::Widths;
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
    let listed = history.downgrade();
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
