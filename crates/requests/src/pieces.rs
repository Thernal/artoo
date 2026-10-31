//! The requests' pieces of the item page (#165 I5c): over it while a request is being written, its header, and its
//! switch at the top of the diff's column; on a request's own page once its diff is shown, its line. They read the review's diff and the tab's answers to the
//! requests' acts from the tab's services.

use crate::create::Totals;
use crate::requests::{Acts, Requests};
use artoo_core::route::Place;
use artoo_requests_api::RequestRoute;
use artoo_review_api::Review;
use artoo_review_api::diff::Header;
use artoo_shell_api::{Piece, Services, Slot};
use artoo_ui_kit::markdown;
use gpui::{Entity, prelude::*};
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

/// A request's line over its Commits and Changes (`Requests::request_line`): on its page, once the diff shown is
/// the request's own; its title selectable.
pub fn line(requests: Entity<Requests>, services: Rc<Services>) -> Piece {
    Piece {
        id: "requests.line",
        slot: Slot::Identity,
        draw: Rc::new(move |route, drawing, cx| {
            let RequestRoute { number } = RequestRoute::from_route(route)?;
            let diff = services.get::<Rc<dyn Review>>()?.shown(cx)?;
            if !matches!(diff.header, Header::Compare { request: Some(iid), .. } if iid == number) {
                return None;
            }
            let m = {
                let rq = requests.read(cx);
                rq.find(number)
                    .filter(|_| rq.current == Some(number))?
                    .clone()
            };
            let acts = services.get::<Acts>()?;
            let title = markdown::body(
                &m.title,
                false,
                format!("line-title-{number}"),
                None,
                Some(drawing.selecting.clone()),
                cx,
            );
            Some(
                requests
                    .read(cx)
                    .request_line(&m, title, acts)
                    .into_any_element(),
            )
        }),
    }
}
