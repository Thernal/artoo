//! The Inbox, a place of the menu (#49), as the requests' page (Design L4, STYLE.md: "notifications listed, the
//! selected one previewed and answerable"): the notifications on the left in one list, each two lines with a face or
//! its source's mark (#197), the one selected on the right (`inbox_view`), its freshness in the head's slot (round 41);
//! Source ▾'s list over it while open. Among them your notes an
//! agent answered (#116): which they are the agents say (`Agents::inbox_answers`, in the tab's services); their rows
//! and previews, the composer and the answers to the clicks come through the tab's `InboxHost`.

use crate::inbox::{InboxRow, InboxView};
use crate::inbox_view::{self, NoticeLook, OnInbox, RowLook, notice_preview, notice_row};
use crate::notices::Notices;
use crate::requests::Requests;
use artoo_agents_api::tab::Agents;
use artoo_shell_api::{Frame, Pages, Screen};
use artoo_ui_kit::room::Heights;
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{island, motion, px, theme};
use gpui::{
    AnyElement, App, Context, Entity, MouseButton, Render, SharedString, Window, anchored,
    deferred, div, point, prelude::*,
};
use std::rc::Rc;
use std::time::Instant;

pub use artoo_requests_api::InboxHost;

/// The Inbox in one tab.
pub struct InboxPage {
    notices: Entity<Notices>,
    view: Entity<InboxView>,
    requests: Entity<Requests>,
    frame: Rc<dyn Frame>,
    /// The list's width as last drawn: the room its rows have (#79).
    widths: Widths,
    /// The heights of rows just done, as last drawn: what their gaps close from.
    heights: Heights,
}

/// The Inbox's page in a tab.
pub fn pages(
    notices: Entity<Notices>,
    view: Entity<InboxView>,
    requests: Entity<Requests>,
    frame: Rc<dyn Frame>,
    cx: &mut App,
) -> Pages {
    let services = Rc::downgrade(&frame.services());
    let page = cx.new(|cx| {
        cx.observe(&notices, |_, _, cx| cx.notify()).detach();
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        cx.observe(&requests, |_, _, cx| cx.notify()).detach();
        InboxPage {
            notices,
            view,
            requests,
            frame,
            widths: Widths::default(),
            heights: Heights::default(),
        }
    });
    Pages {
        space: artoo_requests_api::INBOX,
        kinds: &[],
        parent: |_| None,
        item: |_| None,
        path: Rc::new(|_, _, _| None),
        screen: |_| Screen::Own,
        row: Rc::new(|_, _, _| None),
        called: |_| "this merge request",
        // The request of the notice picked.
        link: Rc::new(move |_, web, _, cx| {
            let services = services
                .upgrade()
                .ok_or_else(|| "Nothing here has a link".to_owned())?;
            crate::link::inbox_link(web, &services, cx)
        }),
        view: page.into(),
        side: None,
        show: Rc::new(|_, _, _| {}),
        keys: None,
        here: None,
        bar: None,
    }
}

impl Render for InboxPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A row done is drawn until its gap has closed, and the page once more without it.
        if !self.notices.read(cx).leaving(Instant::now()).is_empty() {
            window.request_animation_frame();
        }
        // A column: the page is stretched across to its width, whatever its content's least width.
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.page(cx))
            .children(self.sources(window, cx))
    }
}

impl InboxPage {
    /// Its panes, or the first read's tints over them, or one message when nothing is listed. A read that failed is
    /// said by the head's slot (round 41); with nothing listed, by the message too.
    fn page(&self, cx: &mut Context<Self>) -> AnyElement {
        let services = self.frame.services();
        let Some(host) = services.get::<Rc<dyn InboxHost>>() else {
            return div().into_any_element();
        };
        let on: OnInbox = {
            let host = host.clone();
            Rc::new(move |event, cx| host.act(event, cx))
        };
        // The list's handle first: the tab draws it in its own update.
        let handle = (!host.narrow(cx)).then(|| self.frame.list_handle(cx));
        let cx: &App = cx;
        let answers = (services.get::<Rc<dyn Agents>>())
            .map(|a| a.inbox_answers(cx))
            .unwrap_or_default();
        let parts = crate::inbox::shown_parts(&services, self.view.read(cx).all, cx);
        let (notices, view) = (self.notices.read(cx), self.view.read(cx));
        // The text in the bar's Filter (#197).
        let query = self.frame.filter(cx);
        let rows = view.rows(notices, &answers, &parts, &query, cx);
        // Nothing listed because a read failed is never "nothing" (#185): the message says why.
        let requests = self.requests.read(cx);
        let failed = if requests.error.is_some() {
            Some("Couldn't read the merge requests")
        } else {
            notices.failed.then_some("Couldn't read the discussions")
        };
        // Only a host is waited for: without one (no remote, no token, no git) the Inbox has its plugins' and
        // agents' news alone, at once.
        let waiting = requests.available && !notices.loaded;
        let body = if waiting && rows.is_empty() && (notices.reading() || failed.is_none()) {
            div().flex_1().into_any_element()
        } else if let Some(failed) = failed.filter(|_| rows.is_empty()) {
            inbox_view::failed(failed, requests.error.clone().unwrap_or_default())
        } else if rows.is_empty() && notices.leaving(Instant::now()).is_empty() {
            let others = !notices.list.is_empty() || !answers.is_empty() || !parts.is_empty();
            let source = (view.filter.source)
                .and_then(|id| {
                    (artoo_shell_api::notices::sources(cx).into_iter()).find(|s| s.id == id)
                })
                .map(|s| crate::inbox::source_name(&s, self.host(cx)));
            inbox_view::empty(view.filter, source.as_deref(), &query, others, &on)
        } else {
            self.panes(&rows, &answers, &parts, &host, &on, handle, cx)
        };
        // The first read's tints, over the panes' places; a plugin's rows already there stay in sight.
        let now = Instant::now();
        let clock = &notices.current.clock;
        let phase = clock.phase(now);
        let first = requests.available && !(waiting && !rows.is_empty());
        let tints = (first && phase.skeleton > 0.).then(|| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .flex_col()
                .opacity(phase.skeleton)
                .child(inbox_view::loading(self.frame.list_width(cx)))
        });
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(first && phase.content < 1., |s| s.opacity(phase.content))
                    .child(body),
            )
            .children(tints)
            .when(first && clock.skeleton_moves(now), |s| {
                s.child(crate::freshness::frames())
            })
            .into_any_element()
    }

    /// The host's name, the requests' notices' source: `GitLab`.
    fn host(&self, cx: &App) -> Option<&'static str> {
        self.requests.read(cx).web.as_ref().map(|w| w.name())
    }

    /// Source ▾'s list while it is open, under it (#197), over a backdrop a press on closes it.
    fn sources(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let services = self.frame.services();
        let Some(host) = services.get::<Rc<dyn InboxHost>>() else {
            return Vec::new();
        };
        let cx: &App = cx;
        let view = self.view.read(cx);
        let (Some(pick), Some(at)) = (view.sources.as_ref(), view.source_at.get()) else {
            return Vec::new();
        };
        let on: OnInbox = Rc::new(move |event, cx| host.act(event, cx));
        let answers = (services.get::<Rc<dyn Agents>>())
            .map(|a| a.inbox_answers(cx))
            .unwrap_or_default();
        let parts = crate::inbox::shown_parts(&services, self.view.read(cx).all, cx);
        let counts = view.counts(self.notices.read(cx), &answers, &parts, cx);
        let sources = crate::inbox::sources(self.host(cx), &counts, cx);
        let card = inbox_view::source_picker(
            &self.view,
            pick,
            &sources,
            view.filter,
            counts.everywhere,
            &on,
        );
        let size = window.viewport_size();
        let close = self.view.clone();
        vec![
            deferred(
                anchored()
                    .position(point(gpui::px(0.), gpui::px(0.)))
                    .child(
                        div()
                            .id("inbox-sources-backdrop")
                            .w(size.width)
                            .h(size.height)
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                inbox_view::close_sources(&close, cx)
                            }),
                    ),
            )
            .into_any_element(),
            deferred(
                anchored()
                    .position(point(at.left(), at.bottom() + px(4.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(artoo_shell_api::input_box(
                        div().occlude().child(card),
                        crate::inbox::SOURCES,
                        &self.frame,
                    )),
            )
            .with_priority(1)
            .into_any_element(),
        ]
    }

    /// The list, and the row selected previewed and answerable.
    #[allow(clippy::too_many_arguments)]
    fn panes(
        &self,
        rows: &[InboxRow],
        answers: &[artoo_core::notice::Answer],
        parts: &[artoo_shell_api::notices::Listed],
        host: &Rc<dyn InboxHost>,
        on: &OnInbox,
        handle: Option<AnyElement>,
        cx: &App,
    ) -> AnyElement {
        let (notices, view) = (self.notices.read(cx), self.view.read(cx));
        let current = view.current(notices, answers, parts, cx);
        let part =
            |id: u64| (parts.iter()).find(|r| artoo_shell_api::notices::row_id(&r.key) == id);
        // A part's row names its source (`Jira`), as the registry has it.
        let source = |r: &artoo_shell_api::notices::Listed| {
            artoo_shell_api::notices::kind(cx, r.kind).map_or("", |(_, s)| s.name)
        };
        // Its mark and its kind's badge, for a row with no face of its own.
        let marked = |r: &artoo_shell_api::notices::Listed| {
            artoo_shell_api::notices::kind(cx, r.kind)
                .map(|(k, s)| (inbox_view::source_mark(s.name, s.mark), k.badge))
                .unwrap_or_default()
        };
        let wide = self.widths.room("inbox").second();
        // The requests' notices' source is their host (`GitLab`).
        let named = self.host(cx).unwrap_or("Merge requests");
        let mut list: Vec<AnyElement> = Vec::new();
        let now = Instant::now();
        // The rows just done, each drawn before the first row older than it.
        let mut gone = notices.leaving(now).into_iter().peekable();
        let gone_row = |n| inbox_view::gone_row(n, named, wide, &self.heights);
        // One list in time's order: a row read keeps its place (Design round 10, U4).
        for &row in rows {
            let at = match row {
                InboxRow::Notice(ix) => notices.list.get(ix).map(|n| n.created),
                InboxRow::Agent(id) => (answers.iter()).find(|a| a.id == id).map(|a| a.at),
                InboxRow::Part(id) => part(id).map(|r| r.at),
            };
            while let Some(n) = gone.next_if(|n| at.is_some_and(|at| n.created >= at)) {
                list.push(gone_row(n));
            }
            let picked = current == Some(row);
            list.push(match row {
                InboxRow::Notice(ix) => {
                    let look = RowLook {
                        fresh: notices.unread_at(ix),
                        on: picked,
                        wide,
                    };
                    let n = &notices.list[ix];
                    let row = notice_row(n, ix, named, look, on);
                    // A notice that just landed fades in where it is.
                    if notices.landed(&n.key, now) {
                        motion::appear(
                            row,
                            SharedString::from(format!("inbox-landed-{}", n.key)),
                            motion::SLOT_FADE,
                            |row, t| row.opacity(t),
                        )
                        .into_any_element()
                    } else {
                        row.into_any_element()
                    }
                }
                InboxRow::Agent(id) => {
                    let look = RowLook {
                        fresh: (answers.iter()).any(|a| a.id == id && !notices.is_read(&a.key)),
                        on: picked,
                        wide,
                    };
                    host.answer_row(id, look, cx)
                }
                InboxRow::Part(id) => match part(id) {
                    Some(r) => {
                        let look = RowLook {
                            fresh: notices.part_fresh(r),
                            on: picked,
                            wide,
                        };
                        let (mark, badge) = marked(r);
                        inbox_view::part_row(r, source(r), &mark, badge, look, on)
                            .into_any_element()
                    }
                    None => div().into_any_element(),
                },
            });
        }
        list.extend(gone.map(gone_row));
        let preview = current
            .filter(|r| rows.contains(r))
            .and_then(|r| match r {
                InboxRow::Notice(ix) => {
                    let n = notices.list.get(ix)?;
                    // A review request has nothing to answer: no composer is drawn for it.
                    let answerable = n.kind.answerable();
                    let rq = self.requests.read(cx);
                    let look = NoticeLook {
                        ix: notices.at,
                        request: rq.find(n.iid),
                        thread: n.note.and_then(|note| notices.thread_of(n.iid, note)),
                        view,
                        waiting: &rq.waiting,
                        me: rq.login.as_deref().unwrap_or_default(),
                        rich: host.rich(cx),
                        selecting: host.selecting(cx)?,
                        writing: answerable.then(|| host.writing(cx)).flatten(),
                    };
                    Some(notice_preview(n, look, on, cx).into_any_element())
                }
                InboxRow::Agent(id) => host.answer_preview(id, cx),
                InboxRow::Part(id) => {
                    part(id).map(|r| inbox_view::part_preview(r, source(r), &marked(r).0, on))
                }
            })
            .unwrap_or_else(|| {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme::muted())
                    .child("Choose a notification.")
                    .into_any_element()
            });
        let narrow = handle.is_none();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(
                if narrow {
                    island().flex_1().min_w_0()
                } else {
                    island().w(px(self.frame.list_width(cx))).flex_none()
                }
                .relative()
                .child(self.widths.measure("inbox"))
                .child(
                    div()
                        .id("inbox-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .pb(px(6.))
                        .children(list),
                ),
            )
            // Narrow, the list is the page and a notice opens where it points (#80). Its handle widens or narrows
            // the list, kept for every list page (the developer, 2026-10-23); a double click sets it back.
            .children(handle)
            .children((!narrow).then(|| {
                island().flex_1().min_w_0().bg(theme::editor()).child(
                    div()
                        .id("inbox-preview")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(preview),
                )
            }))
            .into_any_element()
    }
}
