//! Branches' own page (#72): the list (`list`) in the tab's frame of a list beside its preview (`Frame::list_page`),
//! the row picked previewed. What other features know of a branch comes from the tab's services — your review of
//! its tip (`Review`), its request (`Requests`) —; a click asks the tab an `Act`. A branch's pages are the tab's
//! for now.

use crate::card::branch_card;
use crate::history::{Acts, History};
use crate::list::{self, Beside, Card, ListLook};
use artoo_requests_api::Requests;
use artoo_review_api::Review;
use artoo_shell_api::{Frame, Pages, Services};
use artoo_ui_kit::px;
use artoo_ui_kit::room::Widths;
use gpui::{App, Context, Entity, Render, Window, div, prelude::*};
use std::rc::Rc;

/// Branches' own page in one tab.
pub struct RootPage {
    history: Entity<History>,
    frame: Rc<dyn Frame>,
    /// The list's width as last drawn: the room its rows have (#79).
    widths: Widths,
}

/// Branches' pages in a tab: their own page, the list.
pub fn pages(history: Entity<History>, frame: Rc<dyn Frame>, cx: &mut App) -> Pages {
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
        kinds: &[],
        view: view.into(),
        side: None,
        show: Rc::new(|_, _, _| {}),
    }
}

impl Render for RootPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
    }
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
