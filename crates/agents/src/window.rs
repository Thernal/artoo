//! The Agents tab (#76, #77, #94, #220): a tool window on the right in Code, Branches and Merge requests, in two
//! levels as a messenger has them — this project's sessions, each with what it is doing in words, those waiting for
//! you first; and one session's conversation, under a head with `‹` back to the list and the switch between the
//! conversation and the comments not sent. Its level and the view are its own; what a click does to a comment is
//! the tab's (`AgentsHost`).

use crate::agents::Chat;
use crate::chats::Chats;
use crate::comments::AgentComments;
use crate::review::{Kind, kind_of};
use crate::sessions::{self, Session};
use crate::talk;
use crate::view::chat_badge;
use artoo_agents_api::view::WINDOW;
/// What the window asks of the tab, the contract's.
pub use artoo_agents_api::view::{Act, AgentsHost};
use artoo_shell_api::{Badge, Frame, ListKey, MenuItem, Tool};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, MouseButton, Render, Window, div,
    prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;

/// Which level of the tab is on screen.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Level {
    /// The one session when there is one, else the list.
    Auto,
    List,
    /// A session's conversation, by session id.
    Chat(String),
}

/// What a conversation's body shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum View {
    Conversation,
    NotSent,
}

/// The Agents tab.
pub struct AgentsWindow {
    chats: Entity<Chats>,
    comments: Entity<AgentComments>,
    frame: Rc<dyn Frame>,
    level: Level,
    view: View,
    /// The session picked in the list (↑ ↓, ↵ opens it).
    row: Option<String>,
    /// The conversation's scroll: it opens at its end, where the newest message is.
    scroll: gpui::ScrollHandle,
    /// How many blocks it showed last, so a new one scrolls it to the end.
    shown: Cell<usize>,
}

/// The Agents window as a tab's tool window.
pub fn tool(
    chats: Entity<Chats>,
    comments: Entity<AgentComments>,
    frame: Rc<dyn Frame>,
    cx: &mut App,
) -> Tool {
    let window = cx.new(|cx| {
        cx.observe(&chats, |_, _, cx| cx.notify()).detach();
        cx.observe(&comments, |_, _, cx| cx.notify()).detach();
        AgentsWindow {
            chats: chats.clone(),
            comments: comments.clone(),
            frame,
            level: Level::Auto,
            view: View::Conversation,
            row: None,
            scroll: gpui::ScrollHandle::new(),
            shown: Cell::new(0),
        }
    });
    let (opened, keys) = (window.clone(), window.clone());
    Tool {
        id: WINDOW,
        icon: "bot",
        tip: "Agents  ⇧⌘A",
        title: "Agents",
        heading: None,
        // Every space but the Inbox: a plugin's writes notes for agents too (Tasks).
        spaces: &[],
        not_in: &["inbox"],
        on_pages: false,
        stripe: true,
        // After what is about the page, before Report a problem.
        order: 20,
        badge: Some(Rc::new(move |cx| badge(&chats, &comments, cx))),
        // The badge's words: what needs you (#201).
        count: None,
        view: window.into(),
        // The chats read again as it opens, once the tab is free to say where.
        opened: Rc::new(move |cx| {
            let window = opened.clone();
            cx.defer(move |cx| window.update(cx, |w, cx| w.refresh_chats(cx)));
        }),
        closed: Rc::new(|_| {}),
        typing: Rc::new(Cell::new(false)),
        key: Rc::new(|_, _, _| {}),
        list_key: Some(Rc::new(move |key, cx| {
            keys.update(cx, |w, cx| w.list_key(key, cx))
        })),
    }
}

/// The stripe's icon: what needs you — the questions an agent asks, the drafts, the notes a gone chat never took —
/// (#201), a dot for each chat listening on its own hue (#77).
fn badge(chats: &Entity<Chats>, comments: &Entity<AgentComments>, cx: &App) -> Badge {
    let chats = chats.read(cx);
    let listening = chats.listening();
    let alive = |s: &str| listening.contains(s);
    let (mut asks, mut drafts, mut waiting) = (0, 0, 0);
    for c in comments.read(cx).list.iter().filter(|c| !c.archived) {
        match kind_of(c, &alive) {
            Kind::Asks => asks += 1,
            Kind::Draft => drafts += 1,
            Kind::Waiting => waiting += 1,
            _ => {}
        }
    }
    let what = [
        (asks > 0).then(|| plural(asks, "question")),
        (drafts > 0).then(|| plural(drafts, "draft")),
        (waiting > 0).then(|| plural(waiting, "note not taken")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    let dots: Vec<_> = chats
        .list
        .iter()
        .filter(|c| c.listening())
        .map(|c| {
            div()
                .size(px(5.))
                .rounded_full()
                .bg(chats.mark(&c.session).1)
        })
        .collect();
    Badge {
        count: asks + drafts + waiting,
        color: theme::agent(),
        what,
        under: Some(div().flex().gap(px(3.)).children(dots).into_any_element()),
    }
}

impl AgentsWindow {
    fn host(&self) -> Option<Rc<dyn AgentsHost>> {
        self.frame.services().get::<Rc<dyn AgentsHost>>()
    }

    /// The project's main working tree, whatever worktree of it is open.
    fn main(&self, cx: &App) -> Option<std::path::PathBuf> {
        let places = self.frame.places(cx);
        places.project.or(places.root)
    }

    /// Reads this repository's chats again (`Chats::refresh`).
    fn refresh_chats(&mut self, cx: &mut Context<Self>) {
        let main = self.main(cx);
        let comments = self.comments.clone();
        self.chats.update(cx, |c, cx| {
            if c.refresh(main.as_deref(), &comments.read(cx).list) {
                cx.notify();
            }
        });
    }

    /// The sessions of the list, in its order.
    fn sessions(&self, cx: &App) -> Vec<Session> {
        let chats = self.chats.read(cx);
        sessions::sessions(
            &chats.list,
            &self.comments.read(cx).list,
            &chats.messages,
            &chats.spoken,
            format::now(),
        )
    }

    /// The session whose conversation is open, if any: the one asked for while it is still listed, the only one
    /// when there is one and the list was not asked for.
    fn open_session<'a>(&self, list: &'a [Session]) -> Option<&'a Session> {
        match &self.level {
            Level::Chat(id) => list.iter().find(|s| s.chat.session == *id),
            Level::Auto if list.len() == 1 => list.first(),
            _ => None,
        }
    }

    /// ↑ ↓ pick a session in the list, ↵ opens it.
    fn list_key(&mut self, key: ListKey, cx: &mut Context<Self>) -> bool {
        let list = self.sessions(cx);
        if self.open_session(&list).is_some() || list.is_empty() {
            return false;
        }
        let at = self
            .row
            .as_ref()
            .and_then(|id| list.iter().position(|s| s.chat.session == *id));
        match key {
            ListKey::Up | ListKey::Down => {
                let to = match (at, key == ListKey::Down) {
                    (Some(i), true) => (i + 1).min(list.len() - 1),
                    (Some(i), false) => i.saturating_sub(1),
                    (None, _) => 0,
                };
                self.row = Some(list[to].chat.session.clone());
            }
            ListKey::Enter => {
                let Some(i) = at else { return false };
                self.level = Level::Chat(list[i].chat.session.clone());
            }
            ListKey::Delete => return false,
        }
        cx.notify();
        true
    }

    /// The time a session's last message is shown at: the clock today, else the date.
    fn when(time: i64) -> String {
        if format::date(time) == format::date(format::now()) {
            format::clock(time)
        } else {
            format::date(time)
        }
    }

    /// A session's mark, hollow once it ended.
    fn avatar(&self, chat: &Chat, ended: bool, size: f32, cx: &App) -> gpui::Div {
        let (letter, hue) = self.chats.read(cx).mark(&chat.session);
        chat_badge(letter, hue, ended)
            .size(px(size))
            .text_size(px(size * 0.5))
    }

    /// A row of the list: the avatar, the name and the time of its last message, under it what it is doing.
    fn row(&self, ix: usize, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let chat = &s.chat;
        let on = self.row.as_deref() == Some(chat.session.as_str());
        let (session, agent) = (chat.session.clone(), chat.agent.clone());
        let named = (session.clone(), agent);
        let said: gpui::SharedString = format!(
            "{}\n{} in {} · last heard {}",
            sessions::name(chat),
            chat.agent,
            format::home_relative(&chat.repo),
            hover::full_date(chat.beat)
        )
        .into();
        let colour = if s.waits() {
            theme::warning()
        } else if s.ended() {
            theme::faint()
        } else {
            theme::muted()
        };
        div()
            .id(("session-row", ix))
            // Its right click names it (the developer, 2026-10-23: chats mixed up).
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    let host = this.host();
                    let (session, agent) = named.clone();
                    this.frame.menu(
                        e.position,
                        vec![vec![MenuItem::new("Name this chat…", move |_, cx| {
                            if let Some(host) = &host {
                                host.act(
                                    Act::NameChat {
                                        session: session.clone(),
                                        agent: agent.clone(),
                                    },
                                    cx,
                                );
                            }
                        })]],
                        cx,
                    )
                }),
            )
            .mx(px(6.))
            .px(px(8.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(on, |d| d.bg(theme::selected()))
            .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.level = Level::Chat(session.clone());
                this.row = Some(session.clone());
                cx.notify();
            }))
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(said.clone())).into())
            .child(self.avatar(chat, s.ended(), 28., cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .cut_in_tip()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(s.ended(), |d| d.text_color(theme::faint()))
                                    .child(sessions::name(chat)),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(Self::when(s.last)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(colour)
                            .children(
                                (s.waits() || matches!(s.doing, sessions::Doing::Working(_))).then(
                                    || {
                                        div().flex_none().size(px(6.)).rounded_full().bg(
                                            if s.waits() {
                                                theme::warning()
                                            } else {
                                                theme::agent()
                                            },
                                        )
                                    },
                                ),
                            )
                            .child(hover::cut(("session-line", ix), s.line())),
                    ),
            )
            .into_any_element()
    }

    /// The list: its head counts the sessions and those waiting; the ended ones come under `Ended today`.
    fn list(&self, list: &[Session], cx: &mut Context<Self>) -> AnyElement {
        let waiting = list.iter().filter(|s| s.waits()).count();
        let mut head = plural(list.len(), "session");
        if waiting > 0 {
            head = format!("{head} · {waiting} wait for you");
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut ended_shown = false;
        for (ix, s) in list.iter().enumerate() {
            if s.ended() && !ended_shown {
                ended_shown = true;
                rows.push(
                    div()
                        .px(px(14.))
                        .pt(px(10.))
                        .pb(px(4.))
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child("Ended today")
                        .into_any_element(),
                );
            }
            rows.push(self.row(ix, s, cx));
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .px(px(14.))
                    .pt(px(8.))
                    .pb(px(6.))
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(head),
            )
            .child(
                div()
                    .id("agents-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }

    /// No session in this project.
    fn empty(&self) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(4.))
            .px(px(24.))
            .child(icons::icon("bot").size(px(24.)).text_color(theme::faint()))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("No agent session yet"),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .text_center()
                    .child("Start an agent in this project. Its session shows up here by itself."),
            )
            .into_any_element()
    }

    /// A session's conversation: its head, the switch, and (until the conversation is drawn) a placeholder.
    fn conversation(&self, s: &Session, others_wait: usize, cx: &mut Context<Self>) -> AnyElement {
        let not_sent = sessions::not_sent(&self.comments.read(cx).list);
        let chats = self.chats.read(cx);
        let talk = artoo_agents_api::conversation::conversation(
            &self.comments.read(cx).list,
            &chats.messages,
            &chats.spoken,
            &s.chat.session,
        );
        let back = div()
            .id("agents-back")
            .flex_none()
            .h(px(28.))
            .px(px(4.))
            .flex()
            .items_center()
            .gap(px(2.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .text_color(theme::muted())
            .hover(|d| d.bg(theme::hover()))
            .tooltip_show_delay(hover::LONG)
            .tooltip(|_, cx| cx.new(|_| hover::Tip("Sessions")).into())
            .child(icons::icon("chevron-left").size(px(16.)))
            .children((others_wait > 0).then(|| {
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::warning())
                    .child(others_wait.to_string())
            }))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.level = Level::List;
                cx.notify();
            }));
        let head = div()
            .flex_none()
            .px(px(8.))
            .pt(px(8.))
            .pb(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(back)
            .child(self.avatar(&s.chat, s.ended(), 28., cx))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .cut_in_tip()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(sessions::name(&s.chat)),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(if s.waits() {
                                theme::warning()
                            } else {
                                theme::muted()
                            })
                            .child(s.head_line()),
                    ),
            );
        let tab = |id: &'static str, label: &'static str, count: usize, on: bool, view: View| {
            div()
                .id(id)
                .h(px(24.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(ROW_RADIUS))
                .text_size(px(12.))
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(theme::selected()).font_weight(FontWeight::SEMIBOLD)
                })
                .when(!on, |d| {
                    d.text_color(theme::muted()).hover(|d| d.bg(theme::hover()))
                })
                .child(label)
                .children((count > 0).then(|| {
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(count.to_string())
                }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.view = view;
                    cx.notify();
                }))
        };
        let switch = div()
            .flex_none()
            .px(px(14.))
            .pb(px(6.))
            .flex()
            .gap(px(4.))
            .child(tab(
                "view-conversation",
                "Conversation",
                0,
                self.view == View::Conversation,
                View::Conversation,
            ))
            .child(tab(
                "view-not-sent",
                "Not sent",
                not_sent,
                self.view == View::NotSent,
                View::NotSent,
            ));
        // Not sent is A5's; until then a count.
        let body = match self.view {
            View::Conversation => self.talk(s, &talk, cx),
            View::NotSent => div()
                .flex_1()
                .min_h_0()
                .px(px(14.))
                .py(px(10.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(format!("{} not sent", plural(not_sent, "comment")))
                .into_any_element(),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .child(switch)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(theme::island_border())
                    .child(body),
            )
            .into_any_element()
    }

    /// The conversation: its blocks in a scrolling column, the newest at the foot.
    fn talk(
        &self,
        s: &Session,
        talk: &[artoo_agents_api::conversation::Entry],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let comments = self.comments.read(cx).list.clone();
        let working = artoo_agents_api::conversation::working_on(&comments, &s.chat.session);
        let blocks = talk::blocks(talk, &comments, &working, format::now());
        let host = self.host();
        let look = talk::Look {
            name: s.chat.agent.clone(),
            mark: self.chats.read(cx).mark(&s.chat.session),
            rich: host.as_ref().is_none_or(|h| h.rich(cx)),
            host,
        };
        let items = talk::view(&blocks, &comments, &look, cx);
        // A new block takes the view to the end, as a messenger does.
        if blocks.len() != self.shown.replace(blocks.len()) {
            let scroll = self.scroll.clone();
            cx.defer(move |_| scroll.scroll_to_bottom());
        }
        div()
            .id("agents-talk")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(20.))
            .children(items)
            .into_any_element()
    }
}

impl Render for AgentsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.sessions(cx);
        let body = if list.is_empty() {
            self.empty()
        } else if let Some(open) = self.open_session(&list) {
            let others = list
                .iter()
                .filter(|s| s.waits() && s.chat.session != open.chat.session)
                .count();
            self.conversation(open, others, cx)
        } else {
            self.list(&list, cx)
        };
        div().flex_1().min_h_0().flex().flex_col().child(body)
    }
}
