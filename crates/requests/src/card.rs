//! A request's card on a long hover over a link to it (Design round 46): its number, its pipeline and the link's
//! glyphs in the head; its title; who wrote it and when it changed; its branches, open discussions and approvals —
//! each said when it is known.

use crate::requests::Requests;
use artoo_core::mr::{Approvals, Mr, Pipeline};
use artoo_shell_api::links::{self, Claim};
use artoo_ui_kit::discussion::avatar;
use artoo_ui_kit::hover::{card_frame, cut, cut_to, name};
use artoo_ui_kit::{format, icons, motion, px, theme};
use gpui::{Context, FontWeight, IntoElement, Render, Window, div, prelude::*};

/// What the card shows, as the requests held it when it opened.
pub(crate) struct Card {
    address: String,
    claim: Claim,
    request: Mr,
    pipeline: Option<Pipeline>,
    approvals: Option<Approvals>,
    /// Its open discussions.
    open: usize,
}

/// Request `iid`'s card for its link at `address`; none when the requests have not read it.
pub(crate) fn of(requests: &Requests, iid: u64, address: &str, claim: Claim) -> Option<Card> {
    let request = requests.find(iid)?.clone();
    // The open request's own reads are newer than its row's.
    let current = requests.current == Some(iid);
    let facts = requests.rows.facts(iid);
    let pipeline = (requests.pipeline.clone().filter(|_| current))
        .or_else(|| facts.and_then(|f| f.pipeline.clone()));
    let approvals = (requests.approvals.clone().filter(|_| current))
        .or_else(|| facts.and_then(|f| f.approvals.clone()));
    let threads = match current {
        true => Some(requests.threads.as_slice()),
        false => facts.map(|f| f.threads.as_slice()),
    };
    let open = threads.map_or(0, |t| crate::list::discussions(t).0);
    Some(Card {
        address: address.to_owned(),
        claim,
        request,
        pipeline,
        approvals,
        open,
    })
}

impl Render for Card {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let m = &self.request;
        let line = || {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_color(theme::muted())
        };
        let head = line()
            .child(
                div()
                    .flex_none()
                    .font_family(theme::code_font())
                    .child(self.claim.name.clone()),
            )
            .children(self.pipeline.as_ref().map(|p| {
                let (glyph, colour) = crate::bar::pipeline_icon(p);
                icons::icon(glyph)
                    .size(px(14.))
                    .flex_none()
                    .text_color(colour)
            }))
            .child(div().flex_1())
            .child(links::glyphs(&self.address, &self.claim));
        let who = line()
            .child(avatar(&m.author, false, 16.))
            .child(cut("request-card-author", m.author.clone()).min_w_0())
            .child(div().flex_1())
            .child(div().flex_none().child(format::short_ago(m.updated)));
        let approvals = (self.approvals.as_ref()).and_then(crate::list::approvals_count);
        let open = self.open;
        let branches = line()
            .child(
                name(
                    "request-card-branches",
                    format!("{} \u{2192} {}", m.source, m.target),
                )
                .min_w_0()
                .font_family(theme::code_font()),
            )
            .child(div().flex_1())
            .children((open > 0).then(|| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        icons::icon("message")
                            .size(px(13.))
                            .text_color(theme::muted()),
                    )
                    .child(open.to_string())
            }))
            .children(approvals.map(|said| div().flex_none().child(said)));
        motion::appear(
            card_frame()
                .w(px(340.))
                .child(head)
                .child(
                    cut_to("request-card-title", m.title.clone(), 2)
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD),
                )
                .child(who)
                .child(branches),
            "card-in",
            motion::FAST,
            |c, v| c.opacity(v),
        )
    }
}
