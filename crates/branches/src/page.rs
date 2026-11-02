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
        space: "branches",
        kinds: &[
            BranchRoute::KIND,
            BranchCommitRoute::KIND,
            BranchCommitsRoute::KIND,
            VersionsRoute::KIND,
            WorkingTreeRoute::KIND,
            CompareRoute::KIND,
        ],
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
