//! The Agents window (#76, #77, #94): a tool window on the right in Code, Branches and Merge requests — the chats
//! here, who listens, who works, who is gone; where a note written now goes; the reviewer's comments to agents in
//! one list by what they wait on, the row picked with what can be done with it. Its filter, the row and the chat
//! picked are its own; what a row's buttons do to a comment, and a comment shown on its line, are the tab's
//! (`AgentsHost`).

use crate::agents::Chat;
use crate::chats::Chats;
use crate::comments::AgentComments;
use crate::review::{self, Comment, State};
use crate::view::{chat_badge, limited};
use artoo_shell_api::{Badge, Frame, ListKey, MenuItem, Tool};
use artoo_ui_kit::controls::{self, group_heading, tool_foot};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover};
use artoo_ui_kit::motion::{self, Folds, Marks};
use artoo_ui_kit::room::{Heights, Widths};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, MouseButton, Render, Window, div,
    prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;

/// Its id: the layout keeps it open by it.
pub const ID: &str = "agents";

/// The chats the Agents window shows before "N more" (#94).
const CHATS_SHOWN: usize = 3;

/// What the Agents window asks of the tab (`Rc<dyn AgentsHost>` in its services).
pub trait AgentsHost {
    /// The branch the page is of, where a note written now goes (#77).
    fn branch(&self, cx: &App) -> Option<String>;

    /// Comments drawn as markdown: a row's text on one line without it.
    fn rich(&self, cx: &App) -> bool;

    /// A click in the window that changes a comment or opens what is the tab's.
    fn act(&self, act: Act, cx: &mut App);
}

/// What a click in the Agents window asks of the tab.
#[derive(Clone, Debug)]
pub enum Act {
    /// Comment `id` shown on its line.
    Show(u64),
    /// These sent, to the chat their route takes.
    Send(Vec<u64>),
    /// The card that sends every draft to the chat picked in it.
    PickChat,
    /// These sent to chat `session` ("" a new chat, opened in a terminal), picked on the card.
    SendTo {
        ids: Vec<u64>,
        session: String,
        title: String,
    },
    /// The card that picks the chat branch `.0`'s notes go to ("" for any).
    Route(String),
    /// A draft written again.
    Edit(u64),
    /// A draft gone, no question asked; the toast's Undo brings it back.
    Delete(u64),
    /// A note its gone chat never took, to the chat its route takes now (#77).
    Resend(u64),
    /// Sent and not taken yet: back to the drafts.
    Withdraw(u64),
    /// Archived, or back from the archive.
    Archive(u64, bool),
    /// Every resolved one archived.
    ArchiveResolved,
    Resolve(u64),
    /// A chat named (the developer, 2026-10-23: chats mixed up).
    NameChat {
        session: String,
        agent: String,
    },
}

/// The Agents window in one tab.
pub struct AgentsWindow {
    chats: Entity<Chats>,
    comments: Entity<AgentComments>,
    frame: Rc<dyn Frame>,
    filter: Filter,
    /// The row picked.
    row: Option<u64>,
    /// The archived ones listed instead.
    archived: bool,
    /// The chat whose notes alone are listed (a click on it), and whether every chat shows rather than the first
    /// few (#94).
    chat: Option<String>,
    all_chats: bool,
    /// The list's width as last drawn: the room its rows have (#79).
    widths: Widths,
    /// What folds open and shut, and when it moved (#110).
    marks: Marks,
    folds: Folds,
    heights: Heights,
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
            filter: Filter::default(),
            row: None,
            archived: false,
            chat: None,
            all_chats: false,
            widths: Widths::default(),
            marks: Marks::default(),
            folds: Folds::default(),
            heights: Heights::default(),
        }
    });
    let (opened, keys) = (window.clone(), window.clone());
    Tool {
        id: ID,
        icon: "bot",
        tip: "Agents  ⇧⌘A",
        title: "Agents",
        spaces: &["code", "branches", "requests"],
        badge: Some(Rc::new(move |cx| badge(&chats, &comments, cx))),
        view: window.into(),
        // The chats read again as it opens, once the tab is free to say where.
        opened: Rc::new(move |cx| {
            let window = opened.clone();
            cx.defer(move |cx| window.update(cx, |w, cx| w.refresh_chats(cx)));
        }),
        typing: Rc::new(Cell::new(false)),
        key: Rc::new(|_, _, _| {}),
        list_key: Some(Rc::new(move |key, cx| {
            keys.update(cx, |w, cx| w.list_key(key, cx))
        })),
    }
}

/// The stripe's icon: the drafts waiting, a dot for each chat listening on its own hue (#77).
fn badge(chats: &Entity<Chats>, comments: &Entity<AgentComments>, cx: &App) -> Badge {
    let drafts = review::drafts(&comments.read(cx).list).len();
    let chats = chats.read(cx);
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
        count: drafts,
        color: theme::agent(),
        what: plural(drafts, "draft"),
        under: Some(div().flex().gap(px(3.)).children(dots).into_any_element()),
    }
}

impl AgentsWindow {
    fn host(&self) -> Option<Rc<dyn AgentsHost>> {
        self.frame.services().get::<Rc<dyn AgentsHost>>()
    }

    /// `act` asked of the tab, from a click; a draft deleted is no longer picked.
    fn act(&mut self, act: Act, cx: &mut App) {
        if let Act::Delete(id) = act
            && self.row == Some(id)
        {
            self.row = None;
        }
        if let Some(host) = self.host() {
            host.act(act, cx);
        }
    }

    /// `act` asked of the tab once its update is over: from a key, which the tab hands in.
    fn act_later(&self, act: Act, cx: &mut App) {
        let host = self.host();
        cx.defer(move |cx| {
            if let Some(host) = host {
                host.act(act, cx);
            }
        });
    }

    /// `key` opened or closed now: drawn on its way, and again once it is still.
    fn mark_motion(&mut self, key: &str, cx: &mut Context<Self>) {
        self.marks.mark(key);
        motion::redraw_after(cx);
    }

    /// Something that folds open and shut (the kit's `folding`): `moving` names the mark its toggle set.
    fn folding(
        &self,
        key: impl Into<String>,
        moving: &str,
        open: bool,
        content: impl IntoElement + 'static,
    ) -> Option<AnyElement> {
        motion::folding(
            &self.folds,
            &self.heights,
            self.marks.recent(moving),
            key,
            open,
            content,
        )
    }

    /// The project's main working tree, whatever worktree of it is open: where a note's route is read.
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

    /// The chat a note on `branch` goes to (`Chats::route_of`).
    fn route_of(&self, branch: Option<&str>, cx: &App) -> Option<Chat> {
        let main = self.main(cx)?;
        self.chats.read(cx).route_of(branch, &main).cloned()
    }

    /// The chat picked, while it is still one of this repository's.
    fn chat_filter<'a>(&'a self, cx: &'a App) -> Option<&'a str> {
        let chats = &self.chats.read(cx).list;
        self.chat
            .as_deref()
            .filter(|s| chats.iter().any(|c| c.session == *s))
    }

    /// Whether comment `c` was sent to the chat picked; any is, with none picked.
    fn of_chat_picked(&self, c: &Comment, cx: &App) -> bool {
        self.chat_filter(cx)
            .is_none_or(|s| c.sent.as_ref().is_some_and(|sent| sent.session == s))
    }

    /// The comments the window lists, newest first: the filter's, or the archived ones.
    fn listed(&self, cx: &App) -> Vec<u64> {
        let listening = self.chats.read(cx).listening();
        let alive = |s: &str| listening.contains(s);
        let mut ids: Vec<u64> = self
            .comments
            .read(cx)
            .list
            .iter()
            .filter(|c| c.archived == self.archived)
            .filter(|c| self.archived || self.filter.takes(kind_of(c, &alive)))
            .filter(|c| self.of_chat_picked(c, cx))
            .map(|c| c.id)
            .collect();
        ids.sort_by_key(|&id| std::cmp::Reverse(id));
        ids
    }

    /// ↑ ↓ ↵ ⌫ the tab hands in.
    fn list_key(&mut self, key: ListKey, cx: &mut Context<Self>) -> bool {
        match key {
            ListKey::Up | ListKey::Down => self.step(key == ListKey::Down, cx),
            ListKey::Enter => self.row_action(cx),
            // ⌫ on the draft picked deletes it; Undo is in its toast (#76).
            ListKey::Delete => {
                let Some(id) = self.row.filter(|id| {
                    self.comments
                        .read(cx)
                        .list
                        .iter()
                        .any(|c| c.id == *id && c.state == State::Draft)
                }) else {
                    return false;
                };
                self.row = None;
                self.act_later(Act::Delete(id), cx);
            }
        }
        cx.notify();
        true
    }

    /// ↑ ↓: another row picked, shown on its line.
    fn step(&mut self, down: bool, cx: &mut Context<Self>) {
        let ids = self.listed(cx);
        let at = self.row.and_then(|id| ids.iter().position(|&i| i == id));
        let to = match (at, down) {
            (Some(i), true) => (i + 1).min(ids.len().saturating_sub(1)),
            (Some(i), false) => i.saturating_sub(1),
            (None, _) => 0,
        };
        if let Some(&id) = ids.get(to) {
            self.mark_motion("agent-row", cx);
            self.row = Some(id);
            self.act_later(Act::Show(id), cx);
        }
    }

    /// ↵: the picked row's first action — Send, Answer, Withdraw, Resolve, Archive.
    fn row_action(&mut self, cx: &mut Context<Self>) {
        let listening = self.chats.read(cx).listening();
        let alive = |s: &str| listening.contains(s);
        let Some((id, kind, archived)) = self
            .row
            .and_then(|id| self.comments.read(cx).list.iter().find(|c| c.id == id))
            .map(|c| (c.id, kind_of(c, &alive), c.archived))
        else {
            return;
        };
        let act = match (archived, kind) {
            (true, _) => Act::Archive(id, false),
            (false, Kind::Draft) => Act::Send(vec![id]),
            (false, Kind::Asks) => Act::Show(id),
            (false, Kind::Sent | Kind::Waiting) => Act::Withdraw(id),
            (false, Kind::Addressed) => Act::Resolve(id),
            (false, Kind::Working | Kind::Resolved) => Act::Archive(id, true),
        };
        self.act_later(act, cx);
    }

    /// A row of the window: its kind as a chip, where it is, its text; picked, what can be done with it (#76).
    fn comment_row(
        &self,
        c: &Comment,
        kind: Kind,
        rich: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = c.id;
        let picked = self.row == Some(id);
        // A folder's note by the folder, with its slash (#97).
        let place = if c.is_folder() {
            c.path.clone()
        } else {
            format!(
                "{}:{}",
                c.path.rsplit('/').next().unwrap_or(&c.path),
                c.line
            )
        };
        let when = c.replies.last().map(|r| r.at).filter(|&at| at > 0);
        // Its anatomy (#79): its state, file:line and the text (two lines) always; when while
        // the window has 260 px; the chat's mark under 360 px leaves first. The tooltip holds the
        // text in full and the chat.
        let room = self.widths.room("agents");
        let chat = c.sent.as_ref().filter(|s| !s.session.is_empty());
        let whole = if c.is_folder() {
            format!("{} — the folder and everything in it", c.path)
        } else {
            format!("{}:{}", c.path, c.line)
        };
        let mut tip = vec![whole, c.body.trim().to_owned()];
        if let Some(s) = chat {
            let to = self
                .chats
                .read(cx)
                .list
                .iter()
                .find(|ch| ch.session == s.session)
                .map(|ch| {
                    if ch.title.is_empty() {
                        ch.agent.clone()
                    } else {
                        ch.title.clone()
                    }
                });
            tip.push(match to {
                Some(to) => format!("to {to}"),
                None => "to a chat that is gone".to_owned(),
            });
        }
        if let Some(at) = when {
            tip.push(hover::full_date(at));
        }
        let mut actions: Vec<AnyElement> = Vec::new();
        // Where a note its gone chat never took would go now (#77).
        let instead = self.route_of(c.branch.as_deref(), cx).map(|chat| {
            if chat.title.is_empty() {
                chat.agent
            } else {
                chat.title
            }
        });
        let mut action = |label: String, primary: bool, act: Act| {
            actions.push(
                row_button(
                    ("agent-act", id as usize * 8 + actions.len()),
                    label,
                    primary,
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.act(act.clone(), cx)
                }))
                .into_any_element(),
            );
        };
        if c.archived {
            action("Unarchive".to_owned(), false, Act::Archive(id, false));
        } else {
            match kind {
                Kind::Draft => {
                    action("Send".to_owned(), true, Act::Send(vec![id]));
                    action("Edit".to_owned(), false, Act::Edit(id));
                    action("Delete".to_owned(), false, Act::Delete(id));
                }
                // Answered where it stands, or put away from here (the developer, 2026-10-23).
                Kind::Asks => {
                    action("Answer…".to_owned(), true, Act::Show(id));
                    action("Resolve".to_owned(), false, Act::Resolve(id));
                    action("Archive".to_owned(), false, Act::Archive(id, true));
                }
                Kind::Sent => action("Withdraw".to_owned(), false, Act::Withdraw(id)),
                // A note a gone chat never took (#77): to the chat its route takes now, or back.
                Kind::Waiting => {
                    if let Some(to) = instead {
                        action(format!("Send to {to} instead"), true, Act::Resend(id));
                    }
                    action("Back to drafts".to_owned(), false, Act::Withdraw(id));
                }
                Kind::Addressed => {
                    action("Resolve".to_owned(), true, Act::Resolve(id));
                    action("Archive".to_owned(), false, Act::Archive(id, true));
                }
                Kind::Working | Kind::Resolved => {
                    action("Archive".to_owned(), false, Act::Archive(id, true))
                }
            }
        }
        let chats = self.chats.read(cx);
        let mark = chat.filter(|_| room.third()).map(|s| {
            let (letter, hue) = chats.mark(&s.session);
            chat_badge(letter, hue, !chats.listens(&s.session))
        });
        div()
            .id(("agent-row", id as usize))
            .px(px(12.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .border_b_1()
            .border_color(theme::island_border())
            .cursor_pointer()
            .when(picked, |s| s.bg(theme::hover()))
            .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(11.))
                    .child(kind_chip(kind))
                    // The chat it went to (#77).
                    .children(mark)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .cut_in_tip()
                            .font_family(theme::code_font())
                            .text_color(theme::muted())
                            .child(place),
                    )
                    .children(when.filter(|_| room.second()).map(|at| {
                        div()
                            .flex_none()
                            .text_color(theme::faint())
                            .child(format::ago(at))
                    }))
                    // A draft goes with × too, no question asked.
                    .children((kind == Kind::Draft && !c.archived).then(|| {
                        div()
                            .id(("agent-delete", id as usize))
                            .flex_none()
                            .size(px(18.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|s| s.bg(theme::selected()))
                            .child(icons::icon("x").size(px(11.)).text_color(theme::faint()))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.act(Act::Delete(id), cx)
                            }))
                    })),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .clamp_in_tip(2)
                    .child(format::one_line(&c.body, rich)),
            )
            // What can be done with it opens under the picked row, in size and alpha (#110).
            .children(
                (!actions.is_empty())
                    .then(|| {
                        self.folding(
                            format!("agent-row-actions-{id}"),
                            "agent-row",
                            picked,
                            div()
                                .pt(px(4.))
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .children(actions),
                        )
                    })
                    .flatten(),
            )
            .tip(tip.join("\n"))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.mark_motion("agent-row", cx);
                this.row = Some(id);
                cx.notify();
                this.act(Act::Show(id), cx)
            }))
            .into_any_element()
    }
}

impl Render for AgentsWindow {
    /// The chats that listen here, then the purple comments by state; under them, the foot.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let host = self.host();
        let rich = host.as_ref().is_some_and(|h| h.rich(cx));
        let comments = self.comments.read(cx).list.clone();
        let drafts = review::drafts(&self.comments.read(cx).list).len();
        // A chat works while it has comments it took and has not answered, and is not listening:
        // the watcher wakes it with them and runs again when it is done.
        let working = |session: &str| {
            comments
                .iter()
                .filter(|c| c.state == State::Sent)
                .filter(|c| {
                    c.sent
                        .as_ref()
                        .is_some_and(|s| s.delivered && s.session == session)
                })
                .filter(|c| c.replies.is_empty() || c.answer.is_some())
                .count()
        };
        // Each chat here, in any worktree: its mark, what it works on, its state and branch (#77).
        // Many of them (#94): those listening, then those working, then the gone, the latest heard
        // first in each; the first few show, the rest under "N more", the one picked always.
        let busy_of = |c: &Chat| {
            if c.listening() {
                0
            } else {
                working(&c.session)
            }
        };
        let chats_here = self.chats.read(cx).list.clone();
        let mut ordered: Vec<&Chat> = chats_here.iter().collect();
        ordered.sort_by_key(|c| {
            let rank = if c.listening() {
                0
            } else if busy_of(c) > 0 {
                1
            } else {
                2
            };
            (rank, std::cmp::Reverse(c.beat))
        });
        let picked_chat = self.chat_filter(cx).map(str::to_owned);
        let hidden = if self.all_chats {
            0
        } else {
            ordered.len().saturating_sub(CHATS_SHOWN)
        };
        // Every chat, each shown or folded away with N more (#110: in size and alpha).
        let shown: Vec<(&Chat, bool)> = ordered
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let shows = i < CHATS_SHOWN
                    || hidden == 0
                    || picked_chat.as_deref() == Some(c.session.as_str());
                (*c, shows)
            })
            .collect();
        let more = (ordered.len() > CHATS_SHOWN).then(|| {
            div()
                .id("chats-more")
                .mx(px(6.))
                .px(px(8.))
                .py(px(4.))
                .rounded(px(ROW_RADIUS))
                .text_size(px(12.))
                .text_color(theme::accent())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(if hidden > 0 {
                    format!(
                        "{hidden} more {} ▾",
                        if hidden == 1 { "chat" } else { "chats" }
                    )
                } else {
                    "Fewer ▴".to_owned()
                })
                .on_click(cx.listener(|this, e: &ClickEvent, _, cx| {
                    if e.click_count() > 1 {
                        return;
                    }
                    this.mark_motion("chats-more", cx);
                    this.all_chats = !this.all_chats;
                    cx.notify();
                }))
        });
        let chats: Vec<_> = shown
            .into_iter()
            .enumerate()
            .filter_map(|(ix, (c, shows))| {
                let busy = busy_of(c);
                let on = picked_chat.as_deref() == Some(c.session.as_str());
                let session = c.session.clone();
                let (letter, hue) = self.chats.read(cx).mark(&c.session);
                let state = if c.listening() {
                    "listening".to_owned()
                } else if busy > 0 {
                    format!("working · {}", plural(busy, "comment"))
                } else {
                    "gone".to_owned()
                };
                // A long hover: the whole title, where it works, when it was last heard (X59).
                let said: gpui::SharedString = format!(
                    "{}\n{} in {} · last heard {}",
                    c.title,
                    c.agent,
                    format::home_relative(&c.repo),
                    hover::full_date(c.beat)
                )
                .into();
                let (named_session, named_agent) = (c.session.clone(), c.agent.clone());
                let row = div()
                    .id(("chat-row", ix))
                    // Its right click names it (the developer, 2026-10-23: chats mixed up).
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                            let host = this.host();
                            let (session, agent) = (named_session.clone(), named_agent.clone());
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
                    .items_start()
                    .gap(px(8.))
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .when(on, |s| s.bg(theme::selected()))
                    .when(!on, |s| s.hover(|s| s.bg(theme::hover())))
                    // A click lists that chat's notes alone; again, every chat's.
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.chat = (this.chat_filter(cx) != Some(session.as_str()))
                            .then(|| session.clone());
                        this.row = None;
                        cx.notify();
                    }))
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(said.clone())).into())
                    .child(chat_badge(letter, hue, !c.listening() && busy == 0))
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
                                            .child(if c.title.is_empty() {
                                                c.agent.clone()
                                            } else {
                                                c.title.clone()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(11.))
                                            .text_color(if c.listening() {
                                                theme::added()
                                            } else if busy > 0 {
                                                theme::agent()
                                            } else {
                                                theme::faint()
                                            })
                                            .child(state),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(c.agent.clone())
                                    .when(c.limited(), |s| {
                                        s.child(limited(gpui::SharedString::from(format!(
                                            "chat-limited-{}",
                                            c.session
                                        ))))
                                    })
                                    .children((!c.branch.is_empty()).then(|| {
                                        div()
                                            .font_family(theme::code_font())
                                            .child(c.branch.clone())
                                    })),
                            ),
                    );
                self.folding(format!("chat-row-{}", c.session), "chats-more", shows, row)
            })
            .collect();
        // Where a note written here goes, and the way to change it (#77).
        let branch = host.as_ref().and_then(|h| h.branch(cx));
        // "Goes to" the chat; why, in its tooltip (#79).
        let route = self.route_of(branch.as_deref(), cx).map(|chat| {
            let (letter, hue) = self.chats.read(cx).mark(&chat.session);
            let on = branch.clone().unwrap_or_default();
            let name = if chat.title.is_empty() {
                chat.agent.clone()
            } else {
                chat.title.clone()
            };
            let routes = &self.chats.read(cx).routes;
            let picked = routes
                .get(on.as_str())
                .or(routes.get(""))
                .is_some_and(|s| *s == chat.session);
            let notes = match &branch {
                Some(b) => format!("Notes on {b} go to {name}"),
                None => format!("Notes go to {name}"),
            };
            let why = if picked {
                format!("{notes}: you picked it.")
            } else if branch.as_deref().is_some_and(|b| chat.branch == b) {
                format!("{notes}, the chat working on that branch.")
            } else {
                format!("{notes}, the project's default chat: none works on that branch.")
            };
            div()
                .id("agents-route")
                .mx(px(14.))
                .mt(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child("Goes to")
                .child(chat_badge(letter, hue, false))
                .child(hover::cut("goes-to", name).text_color(theme::text()))
                .child(
                    div()
                        .id("route-change")
                        .flex_none()
                        .text_color(theme::accent())
                        .cursor_pointer()
                        .child("Change")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.act(Act::Route(on.clone()), cx)
                        })),
                )
                .tip(why)
        });
        // One list, newest first, each row its kind as a chip (#76, B1); the filter narrows it.
        let listening = self.chats.read(cx).listening();
        let alive = |s: &str| listening.contains(s);
        let archived = comments.iter().filter(|c| c.archived).count();
        let listed: Vec<&Comment> = self
            .listed(cx)
            .into_iter()
            .filter_map(|id| comments.iter().find(|c| c.id == id))
            .collect();
        let counts = |f: Filter| {
            comments
                .iter()
                .filter(|c| {
                    !c.archived && f.takes(kind_of(c, &alive)) && self.of_chat_picked(c, cx)
                })
                .count()
        };
        // A narrow window's chips go to a second line rather than past its edge (#80).
        let filters = [Filter::All, Filter::NeedsYou, Filter::Waiting, Filter::Done]
            .map(|f| (f, counts(f)))
            .into_iter()
            .fold(controls::group().flex_wrap(), |group, (f, count)| {
                group.child(
                    controls::chip(
                        f.id(),
                        format!("{} {}", f.label(), count),
                        self.filter == f && !self.archived,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| {
                            this.filter = f;
                            this.archived = false;
                            cx.notify();
                        },
                    )),
                )
            });
        let rows: Vec<_> = listed
            .into_iter()
            .map(|c| self.comment_row(c, kind_of(c, &alive), rich, cx))
            .collect();
        let empty = rows.is_empty();
        let send = (drafts > 0).then(|| {
            let send = div()
                .id("send-to-agent")
                .px_2()
                .h(px(24.))
                .flex()
                .flex_none()
                .items_center()
                .gap_2()
                .rounded(px(ROW_RADIUS))
                .bg(theme::agent())
                .text_color(theme::base())
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .child(if drafts > 1 {
                    format!("Send all {drafts}")
                } else {
                    "Send".to_owned()
                })
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    let ids = review::drafts(&this.comments.read(cx).list);
                    this.act(Act::Send(ids), cx)
                }));
            // The chat it goes to changes rarely: here, beside it.
            let chat = div()
                .id("send-choose-chat")
                .px_2()
                .h(px(24.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .text_size(px(12.))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                .child("Choose the chat…")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.act(Act::PickChat, cx)));
            div().flex().items_center().gap_2().child(send).child(chat)
        });
        let body = div()
            .id("agents-list")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(px(6.))
            .child(self.widths.measure("agents"))
            .child(group_heading(
                match chats_here.len() {
                    0 | 1 => "Chats here".to_owned(),
                    n => format!("Chats here · {n}"),
                },
                None,
            ))
            .children(chats)
            .children(more)
            .children(route)
            .children(chats_here.is_empty().then(|| {
                div()
                    .px(px(14.))
                    .py(px(4.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("No chat listens in this repository. Claude Code starts listening with Artoo agent-watch (Settings → Agents); a Codex session open here listens by itself.")
            }))
            .child(div().px(px(10.)).pt(px(8.)).pb(px(6.)).child(filters))
            .children(rows)
            .children(empty.then(|| {
                div()
                    .px(px(14.))
                    .py(px(10.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(if comments.is_empty() {
                        "No comments for your agent"
                    } else {
                        "Nothing here."
                    })
            }));
        let foot = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .children(send)
            .child(div().flex_1())
            .children(
                comments
                    .iter()
                    .any(|c| !c.archived && c.state == State::Resolved)
                    .then(|| {
                        foot_button("archive-resolved", "Archive resolved".to_owned()).on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.act(Act::ArchiveResolved, cx)
                            }),
                        )
                    }),
            )
            .children((archived > 0 || self.archived).then(|| {
                foot_button(
                    "show-archived",
                    if self.archived {
                        "Back to the list".to_owned()
                    } else {
                        format!("Archived · {archived}")
                    },
                )
                .text_color(theme::accent())
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.archived = !this.archived;
                    this.row = None;
                    cx.notify();
                }))
            }));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(body)
            .child(tool_foot().child(foot))
    }
}

/// Where a comment is in its life with the agent, as the Agents window says it (#76).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The agent asked something and waits for the answer.
    Asks,
    /// Not sent yet, or sent back.
    Draft,
    /// Sent, not taken by its chat yet.
    Sent,
    /// Sent to a chat that is gone, never taken.
    Waiting,
    /// Taken by its chat.
    Working,
    Addressed,
    Resolved,
}

/// `c`'s kind; `alive` says whether a chat (by session) still listens.
pub fn kind_of(c: &Comment, alive: &dyn Fn(&str) -> bool) -> Kind {
    match c.state {
        State::Draft => Kind::Draft,
        State::Sent if !c.replies.is_empty() && c.answer.is_none() => Kind::Asks,
        State::Sent => match &c.sent {
            Some(s) if s.delivered => Kind::Working,
            Some(s) if !s.session.is_empty() && !alive(&s.session) => Kind::Waiting,
            _ => Kind::Sent,
        },
        State::Addressed => Kind::Addressed,
        State::Resolved => Kind::Resolved,
    }
}

/// The Agents window's filters: what needs you (an answer, a send), what waits on the agent, what
/// is done.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Filter {
    #[default]
    All,
    NeedsYou,
    Waiting,
    Done,
}

impl Filter {
    fn takes(self, kind: Kind) -> bool {
        match self {
            Filter::All => true,
            Filter::NeedsYou => matches!(kind, Kind::Asks | Kind::Draft),
            Filter::Waiting => matches!(kind, Kind::Sent | Kind::Waiting | Kind::Working),
            Filter::Done => matches!(kind, Kind::Addressed | Kind::Resolved),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::NeedsYou => "Needs you",
            Filter::Waiting => "Waiting",
            Filter::Done => "Done",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Filter::All => "agents-all",
            Filter::NeedsYou => "agents-needs-you",
            Filter::Waiting => "agents-waiting",
            Filter::Done => "agents-done",
        }
    }
}

/// A kind's chip: the agent's purple for what is the agent's, green for what is done.
fn kind_chip(kind: Kind) -> gpui::Div {
    let (word, color, filled) = match kind {
        Kind::Asks => ("Asks you", theme::agent(), true),
        Kind::Draft => ("Draft", theme::agent(), false),
        Kind::Sent => ("Sent", theme::muted(), false),
        Kind::Waiting => ("Waiting", theme::warning(), false),
        Kind::Working => ("Working", theme::agent(), false),
        Kind::Addressed => ("Addressed", theme::added(), false),
        Kind::Resolved => ("Resolved", theme::added(), true),
    };
    div()
        .flex_none()
        .px(px(6.))
        .rounded(px(4.))
        .border_1()
        .border_color(color)
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(filled, |s| s.bg(color).text_color(theme::base()))
        .when(!filled, |s| s.text_color(color))
        .child(word)
}

/// A button of a picked row: Send filled, the rest plain.
fn row_button(
    id: impl Into<gpui::ElementId>,
    label: String,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .h(px(24.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .text_size(px(12.))
        .cursor_pointer()
        .when(primary, |s| {
            s.bg(theme::agent())
                .text_color(theme::base())
                .font_weight(FontWeight::SEMIBOLD)
        })
        .when(!primary, |s| {
            s.border_1()
                .border_color(theme::island_border())
                .hover(|s| s.bg(theme::selected()))
        })
        .child(label)
}

fn foot_button(id: &'static str, label: String) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .h(px(24.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .text_size(px(12.))
        .text_color(theme::muted())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
        .child(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{Reply, Sent};

    #[test]
    fn a_comment_is_of_the_kind_its_life_says() {
        let alive = |s: &str| s == "chat-1";
        let sent = |session: &str, delivered: bool| Comment {
            state: State::Sent,
            sent: Some(Sent {
                session: session.to_owned(),
                delivered,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(kind_of(&Comment::default(), &alive), Kind::Draft);
        assert_eq!(kind_of(&sent("chat-1", false), &alive), Kind::Sent);
        assert_eq!(kind_of(&sent("gone", false), &alive), Kind::Waiting);
        assert_eq!(kind_of(&sent("chat-1", true), &alive), Kind::Working);
        let asks = Comment {
            replies: vec![Reply::default()],
            ..sent("chat-1", true)
        };
        assert_eq!(kind_of(&asks, &alive), Kind::Asks);
        assert!(Filter::NeedsYou.takes(Kind::Asks) && Filter::NeedsYou.takes(Kind::Draft));
        assert!(Filter::Waiting.takes(Kind::Working) && !Filter::Waiting.takes(Kind::Resolved));
        assert!(Filter::Done.takes(Kind::Addressed) && Filter::All.takes(Kind::Sent));
    }
}
