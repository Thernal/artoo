//! Merge requests' pages (#72): their own, the list grouped by what each needs from you (`list`), the picked one
//! previewed beside it, in the tab's frame of a list page (`Frame::list_page`), or the list's states — loading,
//! empty, an error over it, a host to connect; and a request's — its Overview or Commits (`tabs`), else the review's
//! item page (`artoo_review_api::ItemView`), the branches' side panel beside (`Branches::panel`), what its route
//! shows read on its branch (`Branches::show`). What the tab keeps that the list draws comes through its `RootHost`;
//! your review of each request through the review's handle, the discussions through the notifications; a click asks
//! the tab an `Act`.

use crate::list::{self, ListLook};
use crate::notices::Notices;
use crate::requests::{Acts, Requests};
use crate::tabs::Tabs;
use artoo_branches_api::Branches;
use artoo_core::route::{Place, Route};
use artoo_hosts_api::Mr;
use artoo_requests_api::{
    RequestCommitRoute, RequestCommitsRoute, RequestRoute, RequestVersionsRoute,
};
use artoo_review_api::ItemView;
use artoo_review_api::{Review, Seen};
use artoo_shell_api::{Frame, ItemTab, Pages};
use artoo_ui_kit::controls::skeleton;
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{gap, island, px, theme};
use gpui::{AnyElement, App, Context, Entity, Render, Window, div, prelude::*};
use std::rc::Rc;

/// What Merge requests' own page asks of the tab (`Rc<dyn RootHost>` in its services).
pub trait RootHost {
    /// The panel that connects the project's host, in the list's place while it has no token.
    fn connect(&self, cx: &mut App) -> AnyElement;

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
    let services = Rc::downgrade(&frame.services());
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
        kinds: &[
            RequestRoute::KIND,
            RequestCommitRoute::KIND,
            RequestCommitsRoute::KIND,
            RequestVersionsRoute::KIND,
        ],
        view: view.into(),
        side: Some(Rc::new({
            let services = services.clone();
            move |route, width, cx| {
                route?;
                services
                    .upgrade()?
                    .get::<Rc<dyn Branches>>()?
                    .panel(width, cx)
            }
        })),
        // What a request's route shows is read on its branch.
        show: Rc::new(move |route, _, cx| {
            let branches = services.upgrade().and_then(|s| s.get::<Rc<dyn Branches>>());
            if let (Some(route), Some(branches)) = (route, branches) {
                branches.show(route, cx)
            }
        }),
    }
}

impl Render for RootPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(route) = self.frame.route(cx) {
            return self.item(route, cx);
        }
        // A column: the page is stretched across to its width, whatever its content's least width.
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.page(cx))
            .into_any_element()
    }
}

impl RootPage {
    /// A request's Overview or Commits, else the review's item page in a column the page's width.
    fn item(&self, route: Route, cx: &mut Context<Self>) -> AnyElement {
        let services = self.frame.services();
        if let Some(tab) = self.frame.item_tab(cx)
            && let Some(tabs) = services.get::<Entity<Tabs>>()
        {
            let number = RequestRoute::from_route(&route).map(|r| r.number);
            tabs.update(cx, |t, _| {
                t.number = number;
                t.overview = tab == ItemTab::Overview;
            });
            return tabs.into_any_element();
        }
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .children(services.get::<ItemView>().map(|v| v.0))
            .into_any_element()
    }

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
            // Each one's discussions, as the notifications read them.
            let notices = services.get::<Entity<Notices>>();
            let notices = notices.as_ref().map(|n| n.read(cx));
            let threads = |iid| notices?.threads_of(iid);
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
