//! The Agents window (#76, #77, #94): a tool window on the right in Code, Branches and Merge requests — the chats
//! here, who listens, who works, who is gone; where a note written now goes; the reviewer's comments to agents in
//! one list by what they wait on, the row picked with what can be done with it. Its filter, the row and the chat
//! picked are its own; what a row's buttons do to a comment, and a comment shown on its line, are the tab's
//! (`AgentsHost`).

use crate::agents::Chat;
use crate::chats::Chats;
use crate::comments::AgentComments;
use crate::review::{Comment, Kind, State, kind_of};
use crate::view::{chat_badge, limited, limits_of};
use artoo_agents_api::view::WINDOW;
/// What the window asks of the tab, the contract's.
pub use artoo_agents_api::view::{Act, AgentsHost};
use artoo_shell_api::{Badge, Frame, ListKey, MenuItem, Tool};
use artoo_ui_kit::controls::{group_heading, tool_foot};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover, RowTip};
use artoo_ui_kit::motion::{self, Folds, Marks};
use artoo_ui_kit::room::{Heights, Widths};
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, MouseButton, Render, Window, div,
    prelude::*,
};
use std::cell::Cell;
use std::rc::Rc;

/// The chats the Agents window shows before "N more" (#94).
const CHATS_SHOWN: usize = 3;

/// The Agents window in one tab.
pub struct AgentsWindow {
    chats: Entity<Chats>,
    comments: Entity<AgentComments>,
    frame: Rc<dyn Frame>,
    /// Done folded to its heading (#201).
    done_folded: bool,
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
            done_folded: false,
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
        id: WINDOW,
        icon: "bot",
        tip: "Agents  ⇧⌘A",
        title: "Agents",
        spaces: &["code", "branches", "requests"],
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
        typing: Rc::new(Cell::new(false)),
        key: Rc::new(|_, _, _| {}),
        list_key: Some(Rc::new(move |key, cx| {
            keys.update(cx, |w, cx| w.list_key(key, cx))
        })),
    }
}

/// `act` done when it is the agents' own — a comment withdrawn, archived or back, resolved; the resolved archived —:
/// whether it was. The tab hears the comments changed (`Agents::on_comments`).
fn own(comments: &Entity<AgentComments>, act: &Act, cx: &mut App) -> bool {
    use crate::comments::change;
    match *act {
        Act::Withdraw(id) => change(comments, cx, |c| c.withdraw(id)),
        Act::Archive(id, on) => change(comments, cx, |c| c.change(id, |c| c.archived = on)),
        Act::ArchiveResolved => change(comments, cx, |c| {
            c.archive_resolved();
            true
        }),
        Act::Resolve(id) => change(comments, cx, |c| {
            c.change(id, |c| c.state = State::Resolved)
        }),
        _ => return false,
    };
    true
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

    /// `act` from a click: done here when it is the agents' own, else asked of the tab; a draft deleted is no longer
    /// picked.
    fn act(&mut self, act: Act, cx: &mut App) {
        if let Act::Delete(id) = act
            && self.row == Some(id)
        {
            self.row = None;
        }
        if own(&self.comments, &act, cx) {
            return;
        }
        if let Some(host) = self.host() {
            host.act(act, cx);
        }
    }

    /// `act` done once the tab's update is over: from a key, which the tab hands in.
    fn act_later(&self, act: Act, cx: &mut App) {
        let (host, comments) = (self.host(), self.comments.clone());
        cx.defer(move |cx| {
            if !own(&comments, &act, cx)
                && let Some(host) = host
            {
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
            .filter(|c| self.of_chat_picked(c, cx))
            .filter(|c| {
                self.archived || !self.done_folded || Group::of(kind_of(c, &alive)) != Group::Done
            })
            .map(|c| c.id)
            .collect();
        // By what you owe, newest first in each (#201).
        let group = |id: u64| {
            (self.comments.read(cx).list.iter())
                .find(|c| c.id == id)
                .map_or(Group::Done, |c| Group::of(kind_of(c, &alive)))
        };
        ids.sort_by_key(|&id| (group(id), std::cmp::Reverse(id)));
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
            (false, Kind::Waiting) => Act::Resend(id),
            (false, Kind::Sent) => Act::Show(id),
            (false, Kind::Addressed) => Act::Resolve(id),
            (false, Kind::Working) => Act::Show(id),
            (false, Kind::Resolved) => Act::Archive(id, true),
        };
        self.act_later(act, cx);
    }

    /// A row of the window (#201): the agent's own last words — or yours, while it has said nothing —, two lines at
    /// most; where and how long ago; the one thing to do as its button, the rest in its right click.
    fn comment_row(
        &self,
        c: &Comment,
        kind: Kind,
        rich: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = c.id;
        let picked = self.row == Some(id);
        // A folder's note by the folder, with its slash (#97); a plugin's place by its words (#212).
        let place = c.reads();
        let last = c.replies.last();
        let when = last.map(|r| r.at).filter(|&at| at > 0);
        let theirs = last.is_some();
        let words = last.map_or(c.body.as_str(), |r| r.body.as_str());
        let room = self.widths.room("agents");
        let chat = c.sent.as_ref().filter(|s| !s.session.is_empty());
        let whole = c.reads_whole();
        // The tip holds what the row does not: where in full, your note under the agent's words, the chat.
        let mut tip = vec![whole];
        if theirs {
            tip.push(format!("You: {}", c.body.trim()));
        }
        if let Some(s) = chat {
            let to = (self.chats.read(cx).list.iter())
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
        // Where a note its gone chat never took would go now (#77).
        let instead = self.route_of(c.branch.as_deref(), cx).map(|chat| {
            if chat.title.is_empty() {
                chat.agent
            } else {
                chat.title
            }
        });
        // The one thing to do, on rows that need you (#201).
        let one: Option<(String, Act)> = if c.archived {
            Some(("Unarchive".to_owned(), Act::Archive(id, false)))
        } else {
            match kind {
                Kind::Asks => Some(("Answer".to_owned(), Act::Show(id))),
                Kind::Draft => Some(("Send".to_owned(), Act::Send(vec![id]))),
                Kind::Waiting => Some(match &instead {
                    Some(to) => (format!("Send to {to}"), Act::Resend(id)),
                    None => ("Back to drafts".to_owned(), Act::Withdraw(id)),
                }),
                _ => None,
            }
        };
        // Everything else, in its right click.
        let mut more: Vec<(&'static str, Act)> = Vec::new();
        if !c.archived {
            match kind {
                Kind::Draft => {
                    more.push(("Edit", Act::Edit(id)));
                    more.push(("Delete", Act::Delete(id)));
                }
                Kind::Asks | Kind::Addressed => {
                    more.push(("Resolve", Act::Resolve(id)));
                    more.push(("Archive", Act::Archive(id, true)));
                }
                Kind::Sent => more.push(("Withdraw", Act::Withdraw(id))),
                Kind::Waiting => more.push(("Back to drafts", Act::Withdraw(id))),
                Kind::Working | Kind::Resolved => more.push(("Archive", Act::Archive(id, true))),
            }
        }
        let button = one.map(|(label, act)| {
            row_button(("agent-act", id as usize), label, true)
                .flex_none()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.act(act.clone(), cx)
                }))
        });
        let chats = self.chats.read(cx);
        let mark = chat.filter(|_| room.third()).map(|s| {
            let (letter, hue) = chats.mark(&s.session);
            chat_badge(letter, hue, !chats.listens(&s.session))
        });
        let face = if theirs {
            div()
                .flex_none()
                .size(px(20.))
                .rounded_full()
                .bg(theme::agent_bg())
                .flex()
                .items_center()
                .justify_center()
                .child(icons::icon("bot").size(px(12.)).text_color(theme::agent()))
        } else {
            artoo_ui_kit::discussion::avatar("You", true, 20.)
        };
        let window = cx.entity();
        div()
            .id(("agent-row", id as usize))
            .px(px(12.))
            .py(px(8.))
            .flex()
            .items_start()
            .gap(px(8.))
            .border_b_1()
            .border_color(theme::island_border())
            .cursor_pointer()
            .when(picked, |s| s.bg(theme::hover()))
            .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
            .child(face)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .clamp_in_tip(2)
                            .child(format::one_line(words, rich)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(
                                div()
                                    .min_w_0()
                                    .cut_in_tip()
                                    .font_family(theme::code_font())
                                    .text_color(theme::muted())
                                    .child(place),
                            )
                            .children(when.map(|at| {
                                div().flex_none().child(format!("· {}", format::ago(at)))
                            }))
                            // The chat it went to (#77).
                            .children(mark),
                    ),
            )
            .children(button)
            .row_tip(picked, tip.join("\n"))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                    if more.is_empty() {
                        return;
                    }
                    let items = (more.iter())
                        .map(|(label, act)| {
                            let (window, act) = (window.clone(), act.clone());
                            MenuItem::new(*label, move |_, cx| {
                                window.update(cx, |w, cx| w.act(act.clone(), cx));
                            })
                        })
                        .collect();
                    this.frame.menu(e.position, vec![items], cx)
                }),
            )
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
                                        s.child(limited(
                                            gpui::SharedString::from(format!(
                                                "chat-limited-{}",
                                                c.session
                                            )),
                                            limits_of(&c.agent, cx),
                                        ))
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
        // Grouped by what you owe (#201): Needs you, Working, Done — Done folds to its heading.
        let listening = self.chats.read(cx).listening();
        let alive = |s: &str| listening.contains(s);
        let archived = comments.iter().filter(|c| c.archived).count();
        let listed: Vec<&Comment> = self
            .listed(cx)
            .into_iter()
            .filter_map(|id| comments.iter().find(|c| c.id == id))
            .collect();
        let counted: Vec<Group> = (comments.iter())
            .filter(|c| !c.archived && self.of_chat_picked(c, cx))
            .map(|c| Group::of(kind_of(c, &alive)))
            .collect();
        let count = |g: Group| counted.iter().filter(|&&x| x == g).count();
        let mut rows: Vec<AnyElement> = Vec::new();
        if self.archived {
            rows.extend((listed.iter()).map(|c| self.comment_row(c, kind_of(c, &alive), rich, cx)));
        } else {
            for g in [Group::Needs, Group::Working, Group::Done] {
                let n = count(g);
                if n == 0 {
                    continue;
                }
                let folded = g == Group::Done && self.done_folded;
                rows.push(
                    div()
                        .id(g.label())
                        .flex()
                        .items_center()
                        .when(g == Group::Done, |s| {
                            s.cursor_pointer().on_click(cx.listener(
                                |this, _: &ClickEvent, _, cx| {
                                    this.done_folded = !this.done_folded;
                                    cx.notify();
                                },
                            ))
                        })
                        .child(
                            group_heading(format!("{} {n}", g.label()), None)
                                .flex_1()
                                .text_color(if g == Group::Needs {
                                    theme::agent()
                                } else {
                                    theme::muted()
                                }),
                        )
                        .children((g == Group::Done).then(|| {
                            div().pr(px(14.)).pt(px(6.)).child(
                                icons::icon(if folded { "chevron-right" } else { "chevron" })
                                    .size(px(12.))
                                    .text_color(theme::faint()),
                            )
                        }))
                        .into_any_element(),
                );
                for c in listed.iter().filter(|c| Group::of(kind_of(c, &alive)) == g) {
                    rows.push(self.comment_row(c, kind_of(c, &alive), rich, cx));
                }
            }
        }
        let empty = rows.is_empty();
        let named = self
            .route_of(branch.as_deref(), cx)
            .or_else(|| chats_here.first().cloned());
        let header = {
            let frame = self.frame.clone();
            div()
                .px(px(14.))
                .pt(px(4.))
                .pb(px(2.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .map(|s| match &named {
                    Some(chat) => {
                        let repo = (chat.repo.file_name())
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        let agent = chat.agent.split_whitespace().next().unwrap_or("Agent");
                        s.child(div().flex_none().size(px(7.)).rounded_full().bg(
                            if chat.listening() {
                                theme::added()
                            } else {
                                theme::faint()
                            },
                        ))
                        .child(
                            hover::cut("agents-chat", format!("{agent} · {repo}"))
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                    }
                    None => s.text_color(theme::muted()).child("No chat").child(
                        div()
                            .id("agents-set-up")
                            .text_color(theme::accent())
                            .cursor_pointer()
                            .child("Set up…")
                            .on_click(move |_: &ClickEvent, _, cx| {
                                frame.open_settings(artoo_agents_api::view::SETTINGS, cx)
                            }),
                    ),
                })
        };
        let body = div()
            .id("agents-list")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .py(px(6.))
            .child(self.widths.measure("agents"))
            // The header names the chat a note goes to (#201): `Claude · payments-service`, its dot green while it
            // listens; the others listed only when there are several.
            .child(header)
            .when(chats_here.len() > 1, |s| s.children(chats).children(more))
            .children(route.filter(|_| chats_here.len() > 1))
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
                        "No comments match"
                    })
            }));
        let foot = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
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

/// What a row owes, the window's groups (#201): what needs you (an answer, a send, a note a gone chat never took),
/// what an agent works on, what is done.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Needs,
    Working,
    Done,
}

impl Group {
    fn of(kind: Kind) -> Self {
        match kind {
            Kind::Asks | Kind::Draft | Kind::Waiting => Group::Needs,
            Kind::Sent | Kind::Working => Group::Working,
            Kind::Addressed | Kind::Resolved => Group::Done,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Group::Needs => "Needs you",
            Group::Working => "Working",
            Group::Done => "Done",
        }
    }
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
    use artoo_agents_api::comment::{Reply, Sent};

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
        assert_eq!(Group::of(Kind::Asks), Group::Needs);
        assert_eq!(Group::of(Kind::Draft), Group::Needs);
        assert_eq!(Group::of(Kind::Waiting), Group::Needs);
        assert_eq!(Group::of(Kind::Working), Group::Working);
        assert_eq!(Group::of(Kind::Resolved), Group::Done);
        assert!(Group::Needs < Group::Working && Group::Working < Group::Done);
    }
}
