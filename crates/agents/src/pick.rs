//! The card that asks which chat (#77), a card over the window: where comments go — this repository's chats, those
//! listening first, then Claude Code's recent ones, which get the comments when they listen, and a new chat —, or
//! with a branch, which chat its notes go to (Change… in the Agents window). The tab opens it (`send`, `route`);
//! what sending does to the comments is the tab's (`AgentsHost`), a route picked is the chats'.

use crate::agents::{self, Bus};
use crate::chats::{self, Chats};
use crate::view::limited;
use crate::window::{Act, AgentsHost};
use artoo_core::labels;
use artoo_shell_api::{Frame, Overlay};
use artoo_ui_kit::keys::{self, On};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, hover, px, theme};
use gpui::{
    App, ClickEvent, Context, Entity, FontWeight, KeyDownEvent, MouseButton, Render, Window, div,
    prelude::*,
};
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

/// A chat the card offers.
#[derive(Clone)]
struct PickChat {
    /// Empty for a new chat.
    session: String,
    title: String,
    agent: String,
    listening: bool,
    /// A Codex session the app watches for (#18): limited.
    limited: bool,
}

/// What the card asks.
struct Pick {
    chats: Vec<PickChat>,
    selected: usize,
    /// The comments it sends.
    ids: Vec<u64>,
    /// The branch whose route it picks ("" for any); nothing is sent then.
    route: Option<String>,
}

/// The "send to which chat?" card of one tab.
pub struct SendCard {
    pick: Option<Pick>,
    chats: Entity<Chats>,
    /// Whether it is open, as the shell reads it.
    open: Rc<Cell<bool>>,
}

impl SendCard {
    pub fn new(chats: Entity<Chats>) -> Self {
        Self {
            pick: None,
            chats,
            open: Rc::default(),
        }
    }

    fn set(&mut self, pick: Option<Pick>, cx: &mut Context<Self>) {
        self.open.set(pick.is_some());
        self.pick = pick;
        cx.notify();
    }

    /// Where to send `ids`: the chats of `project`, Claude Code's recent ones in `root`, a new chat.
    pub fn send(&mut self, ids: Vec<u64>, root: &Path, project: &Path, cx: &mut Context<Self>) {
        let mut chats: Vec<PickChat> = Bus::new()
            .map(|bus| chats::named(bus.chats(project)))
            .unwrap_or_default()
            .into_iter()
            .map(|c| PickChat {
                listening: c.listening(),
                limited: c.limited(),
                session: c.session,
                title: c.title,
                agent: c.agent,
            })
            .collect();
        chats.sort_by_key(|c| !c.listening);
        for (session, title) in agents::claude_chats(root) {
            if !chats.iter().any(|c| c.session == session) {
                chats.push(PickChat {
                    title: labels::chat(&session).unwrap_or(title),
                    session,
                    agent: "Claude Code".into(),
                    listening: false,
                    limited: false,
                });
            }
        }
        chats.push(PickChat {
            session: String::new(),
            title: "New chat".into(),
            agent: "Claude Code".into(),
            listening: true,
            limited: false,
        });
        self.set(
            Some(Pick {
                chats,
                selected: 0,
                ids,
                route: None,
            }),
            cx,
        );
    }

    /// Change…: the chats that listen, `now` the one `branch`'s notes go to now; the one picked takes them.
    pub fn route(&mut self, branch: String, now: Option<String>, cx: &mut Context<Self>) {
        let chats: Vec<PickChat> = self
            .chats
            .read(cx)
            .list
            .iter()
            .filter(|c| c.listening())
            .map(|c| PickChat {
                session: c.session.clone(),
                title: c.title.clone(),
                agent: c.agent.clone(),
                listening: true,
                limited: c.limited(),
            })
            .collect();
        let selected = chats
            .iter()
            .position(|c| Some(&c.session) == now.as_ref())
            .unwrap_or(0);
        self.set(
            Some(Pick {
                chats,
                selected,
                ids: Vec::new(),
                route: Some(branch),
            }),
            cx,
        );
    }

    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.set(None, cx);
    }

    /// Chat `ix` picked: the branch's notes go to it, or the comments are sent to it — what the tab answers once
    /// the update under way is over.
    fn pick(&mut self, ix: usize, frame: &Rc<dyn Frame>, cx: &mut Context<Self>) {
        let Some(pick) = self.pick.take() else {
            return;
        };
        self.set(None, cx);
        let Some(chat) = pick.chats.get(ix).cloned() else {
            return;
        };
        if let Some(branch) = pick.route {
            self.chats.update(cx, |c, cx| {
                c.routes.insert(branch, chat.session);
                cx.notify();
            });
            return frame.redraw(cx);
        }
        let title = if chat.session.is_empty() {
            "a new chat".to_owned()
        } else {
            chat.title
        };
        let act = Act::SendTo {
            ids: pick.ids,
            session: chat.session,
            title,
        };
        let host = frame.services().get::<Rc<dyn AgentsHost>>();
        App::defer(cx, move |cx| {
            if let Some(host) = host {
                host.act(act, cx);
            }
        });
    }

    /// A key while it is open: ↑ ↓ choose, ↵ picks, esc cancels.
    fn key(&mut self, event: &KeyDownEvent, frame: &Rc<dyn Frame>, cx: &mut Context<Self>) {
        let Some(pick) = self.pick.as_mut() else {
            return;
        };
        let last = pick.chats.len().saturating_sub(1);
        match event.keystroke.key.as_str() {
            "escape" => return self.close(cx),
            "enter" => {
                let ix = pick.selected;
                return self.pick(ix, frame, cx);
            }
            "down" | "j" | "tab" => pick.selected = (pick.selected + 1).min(last),
            "up" | "k" => pick.selected = pick.selected.saturating_sub(1),
            _ => return,
        }
        cx.notify();
    }
}

/// The card as a tab's card over the window.
pub fn overlay(card: Entity<SendCard>, frame: Rc<dyn Frame>, cx: &mut App) -> Overlay {
    let open = card.read(cx).open.clone();
    let (key, close) = (card.clone(), card.clone());
    let view = cx.new(|cx| {
        cx.observe(&card, |_, _, cx| cx.notify()).detach();
        CardView {
            card,
            frame: frame.clone(),
        }
    });
    Overlay {
        view: view.into(),
        open,
        key: Rc::new(move |event, _, cx| {
            key.update(cx, |c, cx| c.key(event, &frame, cx));
        }),
        close: Rc::new(move |cx| close.update(cx, |c, cx| c.close(cx))),
        poll: None,
    }
}

/// The card drawn.
struct CardView {
    card: Entity<SendCard>,
    frame: Rc<dyn Frame>,
}

impl Render for CardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(pick) = &self.card.read(cx).pick else {
            return div().into_any_element();
        };
        let count = pick.ids.len();
        let heading = match &pick.route {
            Some(b) if !b.is_empty() => format!("Notes on {b} go to"),
            Some(_) => "Notes go to".to_owned(),
            None => format!(
                "Send {count} comment{} to",
                if count == 1 { "" } else { "s" }
            ),
        };
        let (chats, selected) = (pick.chats.clone(), pick.selected);
        let rows = chats.into_iter().enumerate().map(|(ix, chat)| {
            let on = ix == selected;
            let (dot, state) = if chat.session.is_empty() {
                ("+", "opens a terminal".to_owned())
            } else if chat.listening {
                ("●", "listening".to_owned())
            } else {
                ("○", "gets them when it listens".to_owned())
            };
            div()
                .id(("agent-chat", ix))
                .flex()
                .items_center()
                .gap_3()
                .px_3()
                .py_2()
                .rounded(px(ROW_RADIUS))
                .when(on, |s| s.bg(theme::selected()))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(
                    div()
                        .w(px(14.))
                        .text_color(if chat.listening {
                            theme::accent()
                        } else {
                            theme::faint()
                        })
                        .child(dot),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(hover::cut(("pick-chat-title", ix), chat.title.clone()))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .child(format!("{} · {state}", chat.agent))
                                .when(chat.limited, |s| {
                                    s.child(limited(("pick-chat-limited", ix)))
                                }),
                        ),
                )
                .children(on.then(|| keys::caps("↵", On::Plain)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let frame = this.frame.clone();
                    this.card.update(cx, |c, cx| c.pick(ix, &frame, cx));
                }))
        });
        div()
            .id("agent-pick-backdrop")
            .absolute()
            .size_full()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(120.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.card.update(cx, |c, cx| c.close(cx));
                    this.frame.redraw(cx);
                }),
            )
            .child(
                div()
                    .occlude()
                    .w(px(520.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_4()
                    .rounded(px(ISLAND_RADIUS))
                    .border_1()
                    .border_color(theme::island_border())
                    .bg(theme::panel())
                    .shadow_lg()
                    .text_size(px(13.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(heading),
                    )
                    .children(rows)
                    .child(
                        div()
                            .pt_1()
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child(keys::text(
                                "A Claude Code chat listens once it runs Artoo's watcher (Settings → Agents); a Codex session open here listens by itself. ↑↓ choose, esc cancels.",
                            ).into_owned()),
                    ),
            )
            .into_any_element()
    }
}
