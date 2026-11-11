//! The requests' pieces of the item page (#165 I5c): over it while a request is being written, its header, its
//! switch at the top of the diff's column, and its form in the diff's place; over a request's diff reviewed to its
//! last file, the card of what next. They read the review's diff and the tab's
//! answers to the requests' acts from the tab's services.

use crate::create::{self, CreateLook, Totals};
use crate::requests::{Act, Acts, Requests};
use artoo_branches_api::Branches;
use artoo_core::route::{Place, Route};
use artoo_requests_api::{
    RequestCommitRoute, RequestCommitsRoute, RequestRoute, RequestVersionsRoute,
};
use artoo_review_api::Review;
use artoo_shell_api::{Frame, Piece, Services, Slot, input_box};
use artoo_ui_kit::done::{DoneEvent, DoneLook, render_done_card};
use artoo_ui_kit::{island, theme};
use gpui::{App, Entity, prelude::*};
use std::rc::Rc;

/// The header while a request is being written (`Requests::render_create_header`), whatever page it is over.
pub fn create_header(requests: Entity<Requests>, services: Rc<Services>) -> Piece {
    Piece {
        id: "requests.create-header",
        slot: Slot::Identity,
        draw: Rc::new(move |_, _, cx| {
            let totals = services
                .get::<Rc<dyn Review>>()?
                .shown(cx)
                .and_then(|d| Totals::of(&d));
            let acts = services.get::<Acts>()?;
            requests.update(cx, |rq, cx| rq.render_create_header(totals, acts, cx))
        }),
    }
}

/// The Request | Changes switch at the top of the diff's column while a request is being written
/// (`Requests::render_create_switch`), with the count of the files it changes.
pub fn create_switch(requests: Entity<Requests>, services: Rc<Services>) -> Piece {
    Piece {
        id: "requests.create-switch",
        slot: Slot::DiffTop,
        draw: Rc::new(move |_, _, cx| {
            let files = services
                .get::<Rc<dyn Review>>()?
                .shown(cx)
                .map_or(0, |d| d.files.len());
            requests.update(cx, |rq, cx| rq.render_create_switch(files, cx))
        }),
    }
}

/// The form in the diff's place while a request is written there (`Requests::render_create_form`), Request | Changes
/// on top; Changes shows the diff again. A press in it gives it the keyboard, one outside takes it away.
pub fn create_form(requests: Entity<Requests>, frame: Rc<dyn Frame>) -> Piece {
    let services = frame.services();
    Piece {
        id: "requests.create-form",
        slot: Slot::Instead,
        draw: Rc::new(move |_, drawing, cx| {
            let github = {
                let rq = requests.read(cx);
                if !rq.create_in_place() || rq.create_shows_changes() {
                    return None;
                }
                rq.creating.as_ref()?.github
            };
            let shown = services.get::<Rc<dyn Review>>()?.shown(cx);
            let files = shown.as_ref().map_or(0, |d| d.files.len());
            let branches = services.get::<Rc<dyn Branches>>()?.list(cx);
            let acts = services.get::<Acts>()?;
            let look = CreateLook {
                typing: drawing.typing == Some(create::INPUT),
                github,
                base: drawing.web.as_ref().map(|w| w.base.clone()),
                selecting: Some(drawing.selecting.clone()),
                totals: shown.as_ref().and_then(|d| Totals::of(d)),
                branches: &branches,
            };
            let (switch, form) = requests.update(cx, |rq, cx| {
                let switch = rq.render_create_switch(files, cx);
                (switch, rq.render_create_form(&look, acts, cx))
            });
            let form = input_box(form?, create::INPUT, &frame);
            Some(
                island()
                    .flex_1()
                    .min_w_0()
                    .bg(theme::editor())
                    .children(switch)
                    .child(form)
                    .into_any_element(),
            )
        }),
    }
}

/// The request a route of the requests' kinds is of.
fn request_of(route: &Route) -> Option<u64> {
    RequestRoute::from_route(route)
        .map(|r| r.number)
        .or_else(|| RequestCommitRoute::from_route(route).map(|r| r.number))
        .or_else(|| RequestCommitsRoute::from_route(route).map(|r| r.number))
        .or_else(|| RequestVersionsRoute::from_route(route).map(|r| r.number))
}

/// The card over a request's diff once its last file is marked (`artoo_ui_kit::done`): Submit review with what is
/// pending (#188), the next request to review (`Requests::next_request`).
pub fn done(requests: Entity<Requests>, services: Rc<Services>) -> Piece {
    Piece {
        id: "requests.done",
        slot: Slot::OverRows,
        draw: Rc::new(move |route, drawing, cx| {
            let iid = request_of(route)?;
            let review = services.get::<Rc<dyn Review>>()?;
            let files = review.done(cx)?;
            let acts = services.get::<Acts>()?;
            let cx: &App = cx;
            let rq = requests.read(cx);
            let next = rq.next_request(&drawing.filter);
            // Submit review while there is something to send: comments pending, or an approval you may give.
            let pending = rq.drafts.len();
            let may_approve = rq.approvals.as_ref().is_some_and(|a| a.can && !a.mine);
            let look = DoneLook {
                files,
                request: Some(iid),
                submit: (pending > 0 || may_approve).then_some(pending),
                next: next
                    .as_ref()
                    .map(|m| (format!("Next: !{} {}", m.iid, m.title), "⌥↓")),
            };
            let next = next.map(|m| m.iid);
            Some(render_done_card(
                look,
                Rc::new(move |event, cx| match event {
                    DoneEvent::Close => review.close_done(cx),
                    // On Approve while you may: the verdict and what is pending go as one (#188).
                    DoneEvent::Submit => {
                        review.close_done(cx);
                        acts(
                            Act::OpenSubmit {
                                approve: may_approve,
                            },
                            cx,
                        );
                    }
                    DoneEvent::Next => {
                        review.close_done(cx);
                        if let Some(iid) = next {
                            acts(Act::OpenRequest { iid, note: None }, cx);
                        }
                    }
                }),
            ))
        }),
    }
}
