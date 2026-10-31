//! Merge requests' own page (#72): the list grouped by what each needs from you (`list`), the picked one previewed
//! beside it, in the tab's frame of a list page (`Frame::list_page`); or the list's states — loading, empty, an
//! error over it, a host to connect. What the tab keeps that the list draws comes through its `RootHost`; your
//! review of each request through the review's handle; a click asks the tab an `Act`.

use crate::list::{self, ListLook};
use crate::requests::{Acts, Requests};
use artoo_hosts_api::{Mr, Thread};
use artoo_review_api::{Review, Seen};
use artoo_shell_api::{Frame, Pages};
use artoo_ui_kit::controls::skeleton;
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{gap, island, px, theme};
use gpui::{AnyElement, App, Context, Entity, Render, Window, div, prelude::*};
use std::collections::HashMap;
use std::rc::Rc;

/// What Merge requests' own page asks of the tab (`Rc<dyn RootHost>` in its services).
pub trait RootHost {
    /// The panel that connects the project's host, in the list's place while it has no token.
    fn connect(&self, cx: &mut App) -> AnyElement;

    /// Request `iid`'s discussions, as the notifications read them.
    fn threads(&self, iid: u64, cx: &App) -> Option<Vec<Thread>>;

    /// A row folding with its group (#110): by its key, its group's motion and whether it is open; `None` once it
    /// is shut.
    fn fold(
        &self,
        key: String,
        moving: &str,
        open: bool,
        row: AnyElement,
        cx: &App,
    ) -> Option<AnyElement>;

    /// Comments drawn as markdown.
    fn rich(&self, cx: &App) -> bool;

    /// The tab narrow: no preview beside the list, and a click opens a request (#80).
    fn narrow(&self, cx: &App) -> bool;

    /// The next page of the merged or the closed of the scope shown read: the list's end is in sight.
    fn more(&self, cx: &mut App);
}

/// Merge requests' own page in one tab.
pub struct RootPage {
    requests: Entity<Requests>,
    frame: Rc<dyn Frame>,
    /// The list's width as last drawn: the room its rows have (#79).
    widths: Widths,
}

/// Merge requests' pages in a tab: their own page, the list.
pub fn pages(requests: Entity<Requests>, frame: Rc<dyn Frame>, cx: &mut App) -> Pages {
    let view = cx.new(|cx| {
        cx.observe(&requests, |_, _, cx| cx.notify()).detach();
        RootPage {
            requests,
            frame,
            widths: Widths::default(),
        }
    });
    Pages {
        space: "requests",
        kinds: &[],
        view: view.into(),
        side: None,
        show: Rc::new(|_, _, _| {}),
    }
}

impl Render for RootPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A column: the page is stretched across to its width, whatever its content's least width.
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.page(cx))
    }
}

impl RootPage {
    fn page(&self, cx: &mut Context<Self>) -> AnyElement {
        let services = self.frame.services();
        let (Some(host), Some(acts)) = (services.get::<Rc<dyn RootHost>>(), services.get::<Acts>())
        else {
            return div().into_any_element();
        };
        if self.requests.read(cx).connectable {
            return island()
                .flex_1()
                .bg(theme::editor())
                .child(host.connect(cx))
                .into_any_element();
        }
        let review = services.get::<Rc<dyn Review>>();
        let seen_of = |m: &Mr, cx: &App| {
            review.as_ref().map_or(Seen::Never, |r| {
                r.request_seen(m.iid, git2::Oid::from_str(&m.head).ok(), cx)
            })
        };
        let filter = self.frame.filter(cx);
        let (banner, body) = {
            let cx: &App = cx;
            let rq = self.requests.read(cx);
            let shown = rq.shown(&filter, &|m| seen_of(m, cx));
            // How far your review of each got, as the review has it.
            let seen = shown.iter().map(|m| (m.iid, seen_of(m, cx))).collect();
            let parts = shown
                .iter()
                .filter_map(|m| {
                    let head = git2::Oid::from_str(&m.head).ok()?;
                    Some((m.iid, review.as_ref()?.head_part(head, cx)?))
                })
                .collect();
            let threads: HashMap<u64, Vec<Thread>> = shown
                .iter()
                .filter_map(|m| Some((m.iid, host.threads(m.iid, cx)?)))
                .collect();
            let threads = |iid| threads.get(&iid).map(Vec::as_slice);
            let fold = |key, moving: &str, open, row| host.fold(key, moving, open, row, cx);
            let look = ListLook {
                shown: &shown,
                picked: rq.rows.picked,
                filter: &filter,
                compact: host.narrow(cx),
                rich: host.rich(cx),
                widths: &self.widths,
                seen,
                parts,
                threads: &threads,
                fold: &fold,
            };
            (list::banner(rq, &acts), list::list(rq, &look, &acts))
        };
        let body = match body {
            list::Body::Loading => div()
                .flex_1()
                .min_h_0()
                .flex()
                .gap(px(gap()))
                .child(
                    island()
                        .w(px(self.frame.list_width(cx)))
                        .flex_none()
                        .p(px(16.))
                        .child(skeleton("requests-loading", 7)),
                )
                .child(
                    island()
                        .flex_1()
                        .bg(theme::editor())
                        .px(px(28.))
                        .py(px(26.))
                        .child(skeleton("request-preview-loading", 3)),
                )
                .into_any_element(),
            list::Body::Empty(empty) => island()
                .flex_1()
                .bg(theme::editor())
                .child(empty)
                .into_any_element(),
            list::Body::List {
                rows,
                preview,
                more,
            } => {
                if let Some(near) = more {
                    let this = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        if near {
                            host.more(cx);
                        } else {
                            this.update(cx, |_, cx| cx.notify()).ok();
                        }
                    });
                }
                let preview = div()
                    .id("request-preview")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(preview)
                    .into_any_element();
                self.frame.list_page(rows.into_any_element(), preview, cx)
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(gap()))
            .children(banner)
            .child(body)
            .into_any_element()
    }
}
