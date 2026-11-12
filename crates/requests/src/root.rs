//! Merge requests' pages (#72): their own, the list grouped by what each needs from you (`list`), the picked one
//! previewed beside it, in the tab's frame of a list page (`Frame::list_page`), or the list's states — loading,
//! empty, an error over it, a host to connect; and a request's — its Overview or Commits (`tabs`), else the review's
//! item page (`artoo_review_api::ItemView`), the branches' side panel beside (`Branches::panel`), what its route
//! shows read on its branch (`Branches::show`). What the tab keeps that the list draws comes through its `RootHost`;
//! your review of each request through the review's handle, the discussions through the notifications; a click asks
//! the tab an `Act`.

use crate::list::{self, ListLook};
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
use artoo_shell_api::{
    Drawing, Frame, ItemRow, ItemTab, NameHover, PagePath, Pages, PathUp, Screen, Services,
};
use artoo_ui_kit::controls::skeleton;
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{format, gap, island, px, theme, words};
use gpui::{AnyElement, App, Context, Entity, Render, Window, div, prelude::*};
use std::rc::Rc;

pub use artoo_requests_api::RootHost;

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
    let named = requests.downgrade();
    let view = cx.new(|cx| {
        cx.observe(&requests, |_, _, cx| cx.notify()).detach();
        RootPage {
            requests,
            frame,
            widths: Widths::default(),
        }
    });
    Pages {
        space: artoo_requests_api::SPACE,
        kinds: &[
            RequestRoute::KIND,
            RequestCommitRoute::KIND,
            RequestCommitsRoute::KIND,
            RequestVersionsRoute::KIND,
        ],
        parent,
        item,
        path: Rc::new({
            let services = services.clone();
            move |route, drawing, cx| {
                let (requests, services) = (named.upgrade()?, services.upgrade()?);
                path(route, drawing, requests.read(cx), &services, cx)
            }
        }),
        screen,
        row: Rc::new({
            let services = services.clone();
            move |item, _, cx| {
                let services = services.upgrade()?;
                row(item, &services, cx)
            }
        }),
        called: crate::link::called,
        link: Rc::new({
            let services = services.clone();
            move |route, web, diff, cx| {
                let services = services
                    .upgrade()
                    .ok_or_else(|| "Nothing here has a link".to_owned())?;
                crate::link::link(route, web, diff, &services, cx)
            }
        }),
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
        keys: None,
    }
}

impl Render for RootPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .child(self.page(window, cx))
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

    fn page(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
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
            let shown = rq.shown(&filter);
            // How far your review of each got, as the review has it.
            let seen = shown.iter().map(|m| (m.iid, seen_of(m, cx))).collect();
            let parts = shown
                .iter()
                .filter_map(|m| {
                    let head = git2::Oid::from_str(&m.head).ok()?;
                    Some((m.iid, review.as_ref()?.head_part(head, cx)?))
                })
                .collect();
            let fold = |key, moving: &str, open, row| host.fold(key, moving, open, row, cx);
            // A row's second line, measured as the window draws it: 12 px of the interface's face, in `px` units.
            let text_width = |text: &str| {
                let mut style = window.text_style();
                style.font_family = theme::ui_font();
                let run = style.to_run(text.len());
                let line = gpui::SharedString::from(text.to_owned());
                let shaped = window
                    .text_system()
                    .shape_line(line, gpui::px(12.), &[run], None);
                // Each text's box is laid out on whole pixels: rounded up, a part that fits is never cut.
                f32::from(shaped.width).ceil()
            };
            let look = ListLook {
                shown: &shown,
                picked: rq.rows.picked,
                filter: &filter,
                compact: host.narrow(cx),
                widths: &self.widths,
                seen,
                parts,
                fold: &fold,
                text_width: &text_width,
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
                    let requests = services.get::<Rc<dyn artoo_requests_api::Requests>>();
                    cx.defer(move |cx| {
                        if near {
                            if let Some(r) = requests {
                                r.load_past_page(r.scope(cx), cx);
                            }
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

/// The menu's row of a request open under Merge requests: its number and title, said merged or closed since, a
/// notification of it not read yet.
fn row(item: &Route, services: &Services, cx: &App) -> Option<ItemRow> {
    let number = RequestRoute::from_route(item)?.number;
    let requests = services.get::<Rc<dyn artoo_requests_api::Requests>>();
    let m = requests.as_ref().and_then(|r| r.find(number, cx));
    let title = m.as_ref().map(|m| m.title.clone()).unwrap_or_default();
    // One merged or closed since stays here, saying so, until closed (#114).
    let state = m
        .map(|m| m.state)
        .filter(|s| *s != artoo_core::mr::MrState::Opened)
        .map(|s| format!(" · {}", s.word().to_lowercase()))
        .unwrap_or_default();
    let label = format!("{} {title}{state}", words::number(number));
    Some(ItemRow {
        icon: row_icon("pull-request"),
        tip: label.clone(),
        label,
        mono: false,
        news: requests.is_some_and(|r| r.unread_of(number, cx)),
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

/// The bar's path to a request's page: Merge requests, then the request a commit, a run or two versions are of (its
/// page), Commits over a commit or a run (the request's page, where they are picked); the request's identity in the
/// name's long hover, a commit's card over a commit. The request's own page, and what changed since you read it, are
/// `!398` and its title, with nothing above (#188): the title's line over the diff is gone.
fn path(
    route: &Route,
    drawing: &Drawing,
    requests: &Requests,
    services: &Services,
    cx: &App,
) -> Option<PagePath> {
    let branches = || services.get::<Rc<dyn Branches>>();
    let tip = |number: u64| requests.request_tip(number).map(NameHover::Tip);
    let up = |number: u64, commits: bool| {
        let page = Some(RequestRoute { number }.route());
        let mut up = vec![
            PathUp::new("Merge requests", None),
            PathUp::new(words::number(number), page.clone()),
        ];
        if commits {
            up.push(PathUp::new("Commits", page));
        }
        up
    };
    // The request's own page: its number, then its title.
    let own = |number: u64| PagePath {
        up: Vec::new(),
        name: words::number(number),
        mono: false,
        hover: tip(number),
        count: None,
        title: requests.find(number).map(|m| m.title.clone()),
    };
    let (up, name, mono, hover) = match route.kind.as_str() {
        RequestRoute::KIND => {
            let r = RequestRoute::from_route(route)?;
            return Some(own(r.number));
        }
        // A commit is a level under the request's Commits (#73).
        RequestCommitRoute::KIND => {
            let r = RequestCommitRoute::from_route(route)?;
            let id = git2::Oid::from_str(&r.commit).ok()?;
            let card = branches().and_then(|b| b.commit_card(id, drawing.web.as_ref(), cx));
            (
                up(r.number, true),
                format::short(id),
                true,
                card.map(NameHover::Card),
            )
        }
        RequestCommitsRoute::KIND => {
            let r = RequestCommitsRoute::from_route(route)?;
            let oid = |hex: &str| git2::Oid::from_str(hex).ok();
            let (newest, oldest) = (oid(&r.newest)?, oid(&r.oldest)?);
            // How many commits the run is (#208), its ends while they are not among the commits read.
            let at = |id| branches().and_then(|b| b.commit_of(id, cx));
            let (name, mono) = match (at(newest), at(oldest)) {
                (Some(a), Some(b)) if b >= a => (format::plural(b - a + 1, "commit"), false),
                _ => (
                    format!("{}..{}", format::short(oldest), format::short(newest)),
                    true,
                ),
            };
            (up(r.number, true), name, mono, tip(r.number))
        }
        RequestVersionsRoute::KIND => {
            let r = RequestVersionsRoute::from_route(route)?;
            // What changed since you read it is the request's own Changes (#187).
            if (branches().and_then(|b| b.range(Some(route), cx)))
                .is_some_and(|on| on.on == artoo_branches_api::Range::Since)
            {
                return Some(own(r.number));
            }
            let version = |ix| branches()?.version(ix, cx);
            let name = match (version(r.from), version(r.to)) {
                (Some(a), Some(b)) => format!("v{} → v{}", a.number, b.number),
                _ => "versions".to_owned(),
            };
            (up(r.number, false), name, false, tip(r.number))
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

/// The request a commit, a run or two versions of it are under; a request is under Merge requests' own page.
fn parent(route: &Route) -> Option<Route> {
    (route.kind != RequestRoute::KIND)
        .then(|| item(route))
        .flatten()
}

/// The request a page is of, which stays open under Merge requests.
fn item(route: &Route) -> Option<Route> {
    let number = match route.kind.as_str() {
        RequestRoute::KIND => RequestRoute::from_route(route)?.number,
        RequestCommitRoute::KIND => RequestCommitRoute::from_route(route)?.number,
        RequestCommitsRoute::KIND => RequestCommitsRoute::from_route(route)?.number,
        RequestVersionsRoute::KIND => RequestVersionsRoute::from_route(route)?.number,
        _ => return None,
    };
    Some(RequestRoute { number }.route())
}

#[cfg(test)]
mod route_tests {
    use super::*;

    #[test]
    fn a_requests_pages_are_under_it_and_keep_it_open() {
        let request = RequestRoute { number: 398 }.route();
        let commit = RequestCommitRoute {
            number: 398,
            commit: "0123".to_owned(),
        }
        .route();
        let versions = RequestVersionsRoute {
            number: 398,
            from: 1,
            to: 2,
        }
        .route();
        for page in [&commit, &versions] {
            assert_eq!(parent(page), Some(request.clone()));
            assert_eq!(item(page), Some(request.clone()));
        }
        assert_eq!(parent(&request), None);
        assert_eq!(item(&request), Some(request.clone()));
    }
}
