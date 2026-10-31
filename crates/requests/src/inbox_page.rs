//! The Inbox, a place of the menu (#49), as the requests' page (Design L4, STYLE.md: "notifications listed, the
//! selected one previewed and answerable"): the notifications on the left under Today and Earlier, the one selected
//! on the right (`inbox_view`), a banner over them while the host cannot be read (#72). Among them your notes an
//! agent answered (#116), drawn by the agents: what the tab keeps of them, the composer and its answers to the clicks
//! come through its `InboxHost`.

use crate::inbox::{InboxRow, InboxView};
use crate::inbox_view::{
    self, InboxEvent, NoticeLook, OnInbox, RowLook, foot_keys, notice_preview, notice_row,
};
use crate::list;
use crate::notices::{Kind, Notices};
use crate::requests::{Acts, Requests};
use artoo_shell_api::{Frame, Pages};
use artoo_ui_kit::format::{self, now};
use artoo_ui_kit::markdown::Selecting;
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{gap, island, px, theme};
use gpui::{AnyElement, App, Context, Entity, Render, Window, div, prelude::*};
use std::rc::Rc;

/// What the Inbox asks of the tab (`Rc<dyn InboxHost>` in its services).
pub trait InboxHost {
    /// Your notes an agent answered that wait for you: each one's id, when the agent last wrote, and the key read
    /// once it is looked at (a new answer is new again).
    fn answers(&self, cx: &App) -> Vec<(u64, i64, String)>;

    /// The row of the answer to note `id`.
    fn answer_row(&self, id: u64, look: RowLook, cx: &App) -> AnyElement;

    /// The answer to note `id` previewed; `None` once it is gone.
    fn answer_preview(&self, id: u64, cx: &App) -> Option<AnyElement>;

    /// The agent says it is done with note `id`: E resolves it.
    fn answer_done(&self, id: u64, cx: &App) -> bool;

    /// The composer, while it writes a reply to the notice selected.
    fn writing(&self, cx: &App) -> Option<AnyElement>;

    /// Selecting in comments.
    fn selecting(&self, cx: &App) -> Option<Selecting>;

    /// Comments drawn as markdown.
    fn rich(&self, cx: &App) -> bool;

    /// The tab narrow: no preview beside the list, and a click opens a notice (#80).
    fn narrow(&self, cx: &App) -> bool;

    /// A click in the Inbox.
    fn act(&self, event: InboxEvent, cx: &mut App);
}

/// The Inbox in one tab.
pub struct InboxPage {
    notices: Entity<Notices>,
    view: Entity<InboxView>,
    requests: Entity<Requests>,
    frame: Rc<dyn Frame>,
    /// The list's width as last drawn: the room its rows have (#79).
    widths: Widths,
}

/// The Inbox's page in a tab.
pub fn pages(
    notices: Entity<Notices>,
    view: Entity<InboxView>,
    requests: Entity<Requests>,
    frame: Rc<dyn Frame>,
    cx: &mut App,
) -> Pages {
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
        }
    });
    Pages {
        space: "inbox",
        kinds: &[],
        view: page.into(),
        side: None,
        show: Rc::new(|_, _, _| {}),
    }
}

impl Render for InboxPage {
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

impl InboxPage {
    /// Its panes, or rows loading before the first read, or one message when nothing is listed; a banner over it
    /// while the host cannot be read (#72).
    fn page(&self, cx: &mut Context<Self>) -> AnyElement {
        let services = self.frame.services();
        let (Some(host), Some(acts)) =
            (services.get::<Rc<dyn InboxHost>>(), services.get::<Acts>())
        else {
            return div().into_any_element();
        };
        let on: OnInbox = {
            let host = host.clone();
            Rc::new(move |event, cx| host.act(event, cx))
        };
        // The list's handle first: the tab draws it in its own update.
        let handle = (!host.narrow(cx)).then(|| self.frame.list_handle(cx));
        let cx: &App = cx;
        let answers = host.answers(cx);
        let (notices, view) = (self.notices.read(cx), self.view.read(cx));
        let rows = view.rows(notices, &answers);
        let body = if !notices.loaded && rows.is_empty() {
            inbox_view::loading(self.frame.list_width(cx))
        } else if rows.is_empty() {
            let others = !notices.list.is_empty() || !answers.is_empty();
            inbox_view::empty(view.filter, others, &on)
        } else {
            self.panes(&rows, &answers, &host, &on, handle, cx)
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(gap()))
            .children(list::banner(self.requests.read(cx), &acts))
            .child(body)
            .into_any_element()
    }

    /// The list, and the row selected previewed and answerable.
    fn panes(
        &self,
        rows: &[InboxRow],
        answers: &[(u64, i64, String)],
        host: &Rc<dyn InboxHost>,
        on: &OnInbox,
        handle: Option<AnyElement>,
        cx: &App,
    ) -> AnyElement {
        let (notices, view) = (self.notices.read(cx), self.view.read(cx));
        let current = view.current(notices, answers);
        let wide = self.widths.room("inbox").second();
        let today = format::date(now());
        let mut list: Vec<AnyElement> = Vec::new();
        let mut group = None;
        for &row in rows {
            let at = match row {
                InboxRow::Notice(ix) => notices.list[ix].created,
                InboxRow::Agent(id) => answers
                    .iter()
                    .find(|(i, ..)| *i == id)
                    .map_or(0, |(_, at, _)| *at),
            };
            let heading = if format::date(at) == today {
                "Today"
            } else {
                "Earlier"
            };
            if group != Some(heading) {
                group = Some(heading);
                list.push(inbox_view::heading(heading));
            }
            let picked = current == Some(row);
            list.push(match row {
                InboxRow::Notice(ix) => {
                    let look = RowLook {
                        fresh: notices.unread_at(ix),
                        on: picked,
                        wide,
                    };
                    notice_row(&notices.list[ix], ix, look, on).into_any_element()
                }
                InboxRow::Agent(id) => {
                    let look = RowLook {
                        fresh: answers
                            .iter()
                            .any(|(i, _, key)| *i == id && !notices.is_read(key)),
                        on: picked,
                        wide,
                    };
                    host.answer_row(id, look, cx)
                }
            });
        }
        let preview = current
            .filter(|r| rows.contains(r))
            .and_then(|r| match r {
                InboxRow::Notice(ix) => {
                    let n = notices.list.get(ix)?;
                    // A review request has nothing to answer: no composer is drawn for it.
                    let answerable = !matches!(n.kind, Kind::Review | Kind::Todo);
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
        let agent = match current {
            Some(InboxRow::Agent(id)) => Some(host.answer_done(id, cx)),
            _ => None,
        };
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
                island()
                    .flex_1()
                    .min_w_0()
                    .bg(theme::editor())
                    .child(
                        div()
                            .id("inbox-preview")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(preview),
                    )
                    .child(foot_keys(agent))
            }))
            .into_any_element()
    }
}
