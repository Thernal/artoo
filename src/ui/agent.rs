//! Sending the review to an agent's chat (`crate::agents`): the reviewer's comments stay drafts
//! until *Send to agent* (⌥⌘A); a card then lists this repository's chats — those listening first,
//! then Claude Code's recent ones, which get the comments when they listen — and a new chat. The
//! agent's answers come back every couple of seconds, and the reviewer resolves or reopens each.

use super::keys::{self, On};
use super::px;
use super::{ISLAND_RADIUS, ROW_RADIUS, Workspace, theme};
use crate::agents::{self, Bus};
use crate::review::State;
pub(super) use artoo_agents::view::{chat_badge, limited};
use artoo_agents::window::{Act, Kind, kind_of};
use gpui::{
    App, ClickEvent, ClipboardItem, Context, FontWeight, KeyDownEvent, MouseButton, WeakEntity,
    div, prelude::*,
};

/// A chat the card offers.
#[derive(Clone)]
pub struct PickChat {
    /// Empty for a new chat.
    session: String,
    title: String,
    agent: String,
    listening: bool,
    /// A Codex session the app watches for (#18): limited.
    limited: bool,
}

/// The "send to which chat?" card; with `route`, which chat a branch's notes go to (#77).
pub struct AgentPick {
    chats: Vec<PickChat>,
    selected: usize,
    /// The comments it sends.
    ids: Vec<u64>,
    /// The branch whose route it picks ("" for any); nothing is sent then.
    route: Option<String>,
}

impl Workspace {
    fn bus() -> Option<Bus> {
        Bus::new()
    }

    /// The drafts the next sending takes.
    pub(super) fn agent_drafts(&self, cx: &App) -> Vec<u64> {
        self.agent_comments.read(cx).drafts()
    }

    /// Opens the card; without drafts, says so.
    pub(super) fn open_agent_pick(&mut self, cx: &mut Context<Self>) {
        let ids = self.agent_drafts(cx);
        self.open_agent_pick_for(ids, cx);
    }

    /// The chat card for `ids`: where to send them.
    pub(super) fn open_agent_pick_for(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        if ids.is_empty() {
            self.error =
                Some("No comments to send — every comment is already with an agent.".into());
            return cx.notify();
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let project = self.project.clone().unwrap_or_else(|| root.clone());
        let mut chats: Vec<PickChat> = Self::bus()
            .map(|bus| artoo_agents::chats::named(bus.chats(&project)))
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
        for (session, title) in agents::claude_chats(&root) {
            if !chats.iter().any(|c| c.session == session) {
                chats.push(PickChat {
                    title: crate::labels::chat(&session).unwrap_or(title),
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
        self.agent_pick = Some(AgentPick {
            chats,
            selected: 0,
            ids,
            route: None,
        });
        cx.notify();
    }

    /// Change…: the card offers the chats that listen; the one picked takes `branch`'s notes.
    fn open_route_pick(&mut self, branch: String, cx: &mut Context<Self>) {
        let now = self.route_of(Some(&branch), cx).map(|c| c.session);
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
        self.agent_pick = Some(AgentPick {
            chats,
            selected,
            ids: Vec::new(),
            route: Some(branch),
        });
        cx.notify();
    }

    /// Send to … instead: a note its gone chat never took goes to its route now.
    fn resend(&mut self, id: u64, cx: &mut Context<Self>) {
        self.change_comment(id, cx, |c| {
            c.state = State::Draft;
            c.sent = None;
        });
        self.send_agent(vec![id], cx);
    }

    /// Whether a chat is here and listening, as the chats are now: to ask while the tab changes.
    pub(super) fn listening(&self, cx: &App) -> impl Fn(&str) -> bool + use<> {
        let on = self.chats.read(cx).listening();
        move |session| on.contains(session)
    }

    /// Another chat than the page's is changing file `path` (#77). One in a worktree of its own:
    /// the file is changed there (#117: what it really edits). One in this working tree, where
    /// whose edit is whose cannot be told: a note of its on the file was taken and not answered
    /// yet. Its name, letter and hue.
    pub(super) fn changing(&self, path: &str, cx: &App) -> Option<(String, char, gpui::Rgba)> {
        let alive = self.listening(cx);
        let page = self
            .route_of(self.page_branch(cx).as_deref(), cx)
            .map(|c| c.session);
        let chats = self.chats.read(cx);
        let edited = chats
            .list
            .iter()
            .filter(|c| Some(&c.session) != page.as_ref())
            .find(|c| {
                chats
                    .edits
                    .get(&c.session)
                    .is_some_and(|paths| paths.contains(path))
            })
            .map(|c| c.session.clone());
        let session = edited.or_else(|| {
            self.comments(cx)
                .iter()
                .filter(|c| c.path == path && !c.archived && kind_of(c, &alive) == Kind::Working)
                .filter_map(|c| c.sent.as_ref())
                .map(|s| s.session.clone())
                .filter(|s| !chats.edits.contains_key(s))
                .find(|s| Some(s) != page.as_ref())
        })?;
        let chat = chats.list.iter().find(|c| c.session == session)?;
        let (letter, hue) = chats.mark(&session);
        let name = if chat.title.is_empty() {
            chat.agent.clone()
        } else {
            chat.title.clone()
        };
        Some((name, letter, hue))
    }

    /// Sends each of `ids` to its route (#77): the chat working on the branch it was written on,
    /// else the project's default; with no chat listening, the card asks.
    pub(super) fn send_agent(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        if ids.is_empty() {
            return;
        }
        self.refresh_chats(cx);
        let mut by_chat: Vec<(agents::Chat, Vec<u64>)> = Vec::new();
        let mut nowhere = Vec::new();
        for id in ids {
            let branch = self
                .comments(cx)
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.branch.clone());
            match self.route_of(branch.as_deref(), cx) {
                Some(chat) => match by_chat.iter_mut().find(|(c, _)| c.session == chat.session) {
                    Some((_, ids)) => ids.push(id),
                    None => by_chat.push((chat, vec![id])),
                },
                None => nowhere.push(id),
            }
        }
        for (chat, ids) in &by_chat {
            self.change_comments(cx, |list| {
                agents::send(list, ids, &chat.session, &chat.title)
            });
        }
        if !by_chat.is_empty() {
            self.save_comments(cx);
            // A Codex session is handed them now, not on the next beat (#18).
            self.codex_beat(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
        if !nowhere.is_empty() {
            self.open_agent_pick_for(nowhere, cx);
        }
    }

    /// The chat a note on `branch` goes to (`Chats::route_of`), with the one picked for it.
    pub(super) fn route_of(&self, branch: Option<&str>, cx: &App) -> Option<agents::Chat> {
        let main = self.project.as_ref().or(self.root.as_ref())?;
        self.chats.read(cx).route_of(branch, main).cloned()
    }

    /// The branch of the page on screen, as a note written there keeps it: a request's source
    /// branch, a branch's own, else the one checked out (#77).
    pub(super) fn page_branch(&self, cx: &App) -> Option<String> {
        if let Some(iid) = self.requests.read(cx).current
            && let Some(m) = self.requests.read(cx).find(iid)
            && self.nav.space == super::spaces::Space::Requests
        {
            return Some(m.source.clone());
        }
        let shown = match (
            self.nav.space,
            self.history
                .read(cx)
                .branch
                .and_then(|ix| self.history.read(cx).branches.get(ix)),
        ) {
            (super::spaces::Space::Branches, Some(b)) => Some(b),
            _ => self.history.read(cx).branches.iter().find(|b| b.is_head),
        }?;
        // A remote branch is the branch it tracks there.
        Some(match shown.kind {
            crate::git::RefKind::Remote => shown
                .name
                .split_once('/')
                .map_or(shown.name.clone(), |(_, n)| n.to_owned()),
            _ => shown.name.clone(),
        })
    }

    /// The reviewer's pick on the agent's form for comment `id`: saved to the drafts at once
    /// (#10 C) — it goes with the next send.
    pub(super) fn pick_answer(&mut self, id: u64, pick: Option<usize>, cx: &mut Context<Self>) {
        let picked = self.change_comment(id, cx, |c| {
            let form = c.replies.last().and_then(|r| r.form.clone());
            let a = c.answer.get_or_insert_with(|| crate::review::Answered {
                picks: match &form {
                    Some(f) if f.kind == crate::review::FormKind::Order => {
                        (0..f.choices().len()).collect()
                    }
                    Some(f) if f.kind.many() => f
                        .choices()
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| o.on)
                        .map(|(i, _)| i)
                        .collect(),
                    _ => Vec::new(),
                },
                ..Default::default()
            });
            match (pick, &form) {
                (None, _) => {
                    a.your_call = !a.your_call;
                }
                (Some(i), Some(f)) if f.kind.many() => {
                    a.your_call = false;
                    match a.picks.iter().position(|p| *p == i) {
                        Some(at) => {
                            a.picks.remove(at);
                        }
                        None => a.picks.push(i),
                    }
                    a.picks.sort_unstable();
                }
                (Some(i), _) => {
                    a.your_call = false;
                    a.picks = vec![i];
                }
            }
            c.state = State::Draft;
        });
        if picked.is_none() {
            return;
        }
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Order: moves item `at` of the answer one place up (or down).
    pub(super) fn move_answer(&mut self, id: u64, at: usize, up: bool, cx: &mut Context<Self>) {
        let moved = self.change_comment(id, cx, |c| {
            let n = c
                .replies
                .last()
                .and_then(|r| r.form.as_ref())
                .map_or(0, |f| f.choices().len());
            let a = c.answer.get_or_insert_with(|| crate::review::Answered {
                picks: (0..n).collect(),
                ..Default::default()
            });
            let to = if up { at.checked_sub(1) } else { Some(at + 1) };
            let to = to.filter(|&t| t < a.picks.len());
            if let Some(to) = to {
                a.picks.swap(at, to);
                c.state = State::Draft;
            }
            to.is_some()
        });
        if moved == Some(true) {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    /// Under an earlier question of the agent's: its form as it was, what was picked marked, and
    /// the answer in words — or that none was given before it asked again. Only the last question
    /// is answered here (the developer, 2026-10-23: an earlier one's options went).
    pub(super) fn render_form_record(
        &self,
        form: &crate::review::Form,
        answered: Option<&crate::review::Answered>,
    ) -> gpui::AnyElement {
        use crate::review::FormKind;
        let choices = form.choices();
        let picks = answered.map(|a| a.picks.clone()).unwrap_or_default();
        // An order answered is shown in its new order, numbered.
        let rows: Vec<(String, bool, Option<usize>)> = match form.kind {
            FormKind::Order if !picks.is_empty() => picks
                .iter()
                .enumerate()
                .filter_map(|(n, &i)| Some((choices.get(i)?.label.clone(), true, Some(n + 1))))
                .collect(),
            _ => choices
                .iter()
                .enumerate()
                .map(|(i, c)| (c.label.clone(), picks.contains(&i), None))
                .collect(),
        };
        let many = form.kind.many();
        let said = answered
            .map(|a| a.words(Some(form)))
            .filter(|w| !w.trim().is_empty());
        div()
            .ml(px(52.))
            .mr(px(12.))
            .mb(px(8.))
            .px(px(10.))
            .py(px(6.))
            .flex()
            .flex_col()
            .gap(px(3.))
            .rounded(px(ROW_RADIUS))
            .border_1()
            .border_color(theme::island_border())
            .text_size(px(12.))
            .children(rows.into_iter().map(|(label, on, nth)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_color(if on { theme::text() } else { theme::faint() })
                    .child(match nth {
                        Some(n) => div()
                            .w(px(14.))
                            .flex_none()
                            .text_size(px(11.))
                            .child(n.to_string()),
                        None => div()
                            .size(px(10.))
                            .flex_none()
                            .rounded(px(if many { 2. } else { 5. }))
                            .border_1()
                            .border_color(if on { theme::agent() } else { theme::faint() })
                            .when(on, |s| s.bg(theme::agent())),
                    })
                    .child(super::hover::cut(
                        gpui::SharedString::from(format!("answer-{label}")),
                        label,
                    ))
            }))
            .child(
                div()
                    .pt(px(2.))
                    .text_size(px(11.))
                    .text_color(if said.is_some() {
                        theme::muted()
                    } else {
                        theme::faint()
                    })
                    .child(
                        said.unwrap_or_else(|| "Not answered — the agent asked again".to_owned()),
                    ),
            )
            .into_any_element()
    }

    /// Under the agent's question: its form, answered with clicks; the foot sends (#10 C).
    pub(super) fn render_agent_form(
        &self,
        id: u64,
        this: gpui::WeakEntity<Self>,
        cx: &App,
    ) -> Option<gpui::AnyElement> {
        use crate::review::FormKind;
        let c = self.comments(cx).iter().find(|c| c.id == id)?;
        if !matches!(c.state, State::Sent | State::Draft) {
            return None;
        }
        let reply = c.replies.last()?;
        let form = reply.form.as_ref();
        let answer = c.answer.clone().unwrap_or_default();
        let answered = c.answer.is_some();
        let others = self
            .agent_drafts(cx)
            .into_iter()
            .filter(|d| *d != id)
            .count();
        let order: Vec<usize> = match form {
            Some(f) if f.kind == FormKind::Order => {
                if answer.picks.len() == f.choices().len() {
                    answer.picks.clone()
                } else {
                    (0..f.choices().len()).collect()
                }
            }
            Some(f) => (0..f.choices().len()).collect(),
            None => Vec::new(),
        };
        let mark = |on: bool, many: bool| {
            div()
                .size(px(14.))
                .flex_none()
                .rounded(px(if many { 3. } else { 7. }))
                .border_2()
                .border_color(if on { theme::agent() } else { theme::faint() })
                .when(on, |s| s.bg(theme::agent()))
        };
        let rows: Vec<gpui::AnyElement> = match form {
            Some(f) if f.kind == FormKind::Line => {
                self.render_line_choices(c, f, &answer, this.clone(), cx)
            }
            _ => self.render_option_rows(id, form, &order, &answer, answered, &mark, this.clone()),
        };
        // Text: one's own value beside the suggestions (#10 C).
        let own = form.filter(|f| f.kind == FormKind::Text).map(|_| {
            let edit = this.clone();
            div()
                .id(("form-own", id as usize))
                .mx(px(8.))
                .mt(px(2.))
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(ROW_RADIUS))
                .border_1()
                .border_color(theme::agent())
                .bg(theme::base())
                .cursor_text()
                .font_family(theme::code_font())
                .text_size(px(12.))
                .child(if answer.value.is_empty() {
                    div().text_color(theme::faint()).child("Your own value…")
                } else {
                    div().child(answer.value.clone())
                })
                .on_click(move |_, _, cx| {
                    edit.update(cx, |this, cx| this.start_answer_value(id, cx))
                        .ok();
                })
                .into_any_element()
        });
        let (call, note, only, all) = (this.clone(), this.clone(), this.clone(), this);
        let button = |idn: (&'static str, usize), label: String, primary: bool| {
            div()
                .id(idn)
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap_2()
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .text_size(px(12.))
                .when(primary, |s| {
                    s.bg(theme::agent())
                        .text_color(theme::base())
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!primary, |s| {
                    s.border_1()
                        .border_color(theme::island_border())
                        .hover(|s| s.bg(theme::hover()))
                })
                .child(label)
        };
        Some(
            div()
                .mx(px(12.))
                .mb(px(10.))
                .ml(px(52.))
                .p(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(gpui::Rgba {
                    a: 0.35,
                    ..theme::agent()
                })
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(rows)
                .children(own)
                .children((!answer.note.is_empty()).then(|| {
                    div()
                        .px(px(8.))
                        .pt(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!("Your note: {}", answer.note))
                }))
                .child(
                    div()
                        .pt(px(6.))
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            button(("form-call", id as usize), "0  Your call".to_owned(), false)
                                .when(answer.your_call, |s| s.bg(theme::agent_bg()))
                                .on_click(move |_, _, cx| {
                                    call.update(cx, |this, cx| this.pick_answer(id, None, cx))
                                        .ok();
                                }),
                        )
                        .child(
                            button(
                                ("form-note", id as usize),
                                super::keys::text("⇧N  Add a note").into_owned(),
                                false,
                            )
                            .on_click(move |_, _, cx| {
                                note.update(cx, |this, cx| this.start_answer(id, cx)).ok();
                            }),
                        )
                        .children(answered.then(|| {
                            div()
                                .text_size(px(12.))
                                .text_color(theme::agent())
                                .child("✓ In drafts")
                        }))
                        .child(div().flex_1())
                        .children((answered && others > 0).then(|| {
                            button(
                                ("form-only", id as usize),
                                super::keys::text("Send this only  ⌥⌘↵").into_owned(),
                                false,
                            )
                            .on_click(move |_, _, cx| {
                                only.update(cx, |this, cx| this.send_agent(vec![id], cx))
                                    .ok();
                            })
                        }))
                        .children(answered.then(|| {
                            let label = if others > 0 {
                                format!("Send all {}  ⇧⌘↵", others + 1)
                            } else {
                                "Send  ⇧⌘↵".to_owned()
                            };
                            button(
                                ("form-all", id as usize),
                                super::keys::text(&label).into_owned(),
                                true,
                            )
                            .on_click(move |_, _, cx| {
                                all.update(cx, |this, cx| {
                                    let ids = this.agent_drafts(cx);
                                    this.send_agent(ids, cx)
                                })
                                .ok();
                            })
                        })),
                )
                .into_any_element(),
        )
    }

    /// Line: the lines the agent suggests drawn as code, with the lines between them from the
    /// file (when it is the one open); a suggested line is picked with a click (Design
    /// `kinds.html`, line).
    fn render_line_choices(
        &self,
        c: &crate::review::Comment,
        f: &crate::review::Form,
        answer: &crate::review::Answered,
        this: gpui::WeakEntity<Self>,
        cx: &App,
    ) -> Vec<gpui::AnyElement> {
        let id = c.id;
        let choices = f.choices();
        // Each suggestion's line number: the first number in its label.
        let number = |label: &str| -> Option<u32> {
            let digits: String = label
                .chars()
                .skip_while(|ch| !ch.is_ascii_digit())
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse().ok()
        };
        let suggested: Vec<(usize, u32, String)> = choices
            .iter()
            .enumerate()
            .filter_map(|(i, o)| {
                let code = o.code.first().cloned().unwrap_or_else(|| o.detail.clone());
                Some((i, number(&o.label)?, code))
            })
            .collect();
        if suggested.is_empty() {
            return Vec::new();
        }
        let (low, high) = suggested.iter().fold((u32::MAX, 0), |(lo, hi), (_, n, _)| {
            (lo.min(*n), hi.max(*n))
        });
        // The file's own lines fill the gaps when it is the open one.
        let open =
            self.review.read(cx).data.as_ref().filter(|_| {
                self.review.read(cx).current_path().as_deref() == Some(c.path.as_str())
            });
        let text_of = |n: u32| -> Option<String> {
            suggested
                .iter()
                .find(|(_, m, code)| *m == n && !code.is_empty())
                .map(|(_, _, code)| code.clone())
                .or_else(|| {
                    let side = open?.side(false);
                    (n >= 1 && n <= side.count()).then(|| side.line(n).0.to_owned())
                })
        };
        let span = if open.is_some() && high - low <= 12 {
            (low.saturating_sub(1).max(1)..=high + 1).collect::<Vec<u32>>()
        } else {
            suggested.iter().map(|(_, n, _)| *n).collect()
        };
        let rows = span.into_iter().filter_map(|n| {
            let choice = suggested
                .iter()
                .find(|(_, m, _)| *m == n)
                .map(|(i, _, _)| *i);
            // A suggestion stays pickable even when its text is not known here.
            let text = text_of(n).or_else(|| choice.map(|_| String::new()))?;
            let picked = choice.is_some_and(|i| answer.picks.contains(&i));
            let pick = this.clone();
            Some(
                div()
                    .id(("form-line", n as usize))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(8.))
                    .when(picked, |s| {
                        s.bg(theme::agent_bg())
                            .border_l(px(3.))
                            .border_color(theme::agent())
                    })
                    .child(
                        div()
                            .w(px(28.))
                            .flex_none()
                            .text_right()
                            .text_color(if choice.is_some() {
                                theme::agent()
                            } else {
                                theme::faint()
                            })
                            .child(n.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(text),
                    )
                    .children(picked.then(|| {
                        div()
                            .flex_none()
                            .font_family(theme::ui_font())
                            .text_size(px(11.))
                            .text_color(theme::agent())
                            .child("← here")
                    }))
                    .when_some(choice, |s, i| {
                        s.cursor_pointer().hover(|s| s.bg(theme::hover())).on_click(
                            move |_, _, cx| {
                                pick.update(cx, |this, cx| this.pick_answer(id, Some(i), cx))
                                    .ok();
                            },
                        )
                    })
                    .into_any_element(),
            )
        });
        vec![
            div()
                .mx(px(8.))
                .py(px(4.))
                .rounded(px(6.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::editor())
                .font_family(theme::code_font())
                .text_size(px(12.))
                .children(rows)
                .into_any_element(),
            div()
                .px(px(8.))
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(format!(
                    "click a line, or 1–{} for the ones it suggests",
                    suggested.len()
                ))
                .into_any_element(),
        ]
    }

    /// The asked comment the form keys act on: the first one with a form from the top of the
    /// view (#10 C: 1…9 pick, 0 your call, ⇧N a note).
    pub(super) fn acting_agent_form(&self, cx: &App) -> Option<u64> {
        let top = self.review.read(cx).local_top();
        self.review
            .read(cx)
            .rows
            .iter()
            .skip(top)
            .find_map(|row| match row {
                super::rows::Row::Thread(id) => self
                    .comments(cx)
                    .iter()
                    .find(|c| c.id == *id)
                    .filter(|c| matches!(c.state, State::Sent | State::Draft))
                    .filter(|c| c.replies.last().is_some_and(|r| r.form.is_some()))
                    .map(|c| c.id),
                _ => None,
            })
    }

    /// A digit on the acting form: 0 is your call, 1…9 the option drawn with that number (Line:
    /// the suggestion in order). Whether it took the key.
    pub(super) fn answer_by_key(&mut self, digit: usize, cx: &mut Context<Self>) -> bool {
        use crate::review::FormKind;
        let Some(id) = self.acting_agent_form(cx) else {
            return false;
        };
        if digit == 0 {
            self.pick_answer(id, None, cx);
            return true;
        }
        let Some(f) = self
            .comments(cx)
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.replies.last())
            .and_then(|r| r.form.clone())
        else {
            return false;
        };
        if f.kind == FormKind::Order || digit > f.choices().len() {
            return false;
        }
        self.pick_answer(id, Some(digit - 1), cx);
        true
    }

    /// The options of a form, one row each: number, mark, label, detail, code (Order: ↑ ↓).
    #[allow(clippy::too_many_arguments)]
    fn render_option_rows(
        &self,
        id: u64,
        form: Option<&crate::review::Form>,
        order: &[usize],
        answer: &crate::review::Answered,
        answered: bool,
        mark: &dyn Fn(bool, bool) -> gpui::Div,
        this: gpui::WeakEntity<Self>,
    ) -> Vec<gpui::AnyElement> {
        use crate::review::FormKind;
        let Some(f) = form else {
            return Vec::new();
        };

        let choices = f.choices();
        let many = f.kind.many();
        order
            .iter()
            .enumerate()
            .filter_map(|(place, &i)| {
                let o = choices.get(i)?.clone();
                let on = if f.kind == FormKind::Order {
                    false
                } else if answered {
                    answer.picks.contains(&i)
                } else {
                    many && o.on
                };
                let (pick, up, down) = (this.clone(), this.clone(), this.clone());
                let order_kind = f.kind == FormKind::Order;
                let row = div()
                    .id(("form-option", id as usize * 64 + i))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .w(px(16.))
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child((place + 1).to_string()),
                            )
                            .children((!order_kind).then(|| mark(on, many)))
                            .child(div().flex_1().child(o.label.clone()))
                            .children((!o.detail.is_empty()).then(|| {
                                div()
                                    .text_size(px(12.))
                                    .text_color(theme::muted())
                                    .child(o.detail.clone())
                            }))
                            .children(order_kind.then(|| {
                                div()
                                    .flex()
                                    .gap(px(2.))
                                    .child(
                                        div()
                                            .id(("form-up", id as usize * 64 + i))
                                            .px(px(6.))
                                            .rounded(px(4.))
                                            .hover(|s| s.bg(theme::selected()))
                                            .child("↑")
                                            .on_click(move |_, _, cx| {
                                                cx.stop_propagation();
                                                up.update(cx, |this, cx| {
                                                    this.move_answer(id, place, true, cx)
                                                })
                                                .ok();
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id(("form-down", id as usize * 64 + i))
                                            .px(px(6.))
                                            .rounded(px(4.))
                                            .hover(|s| s.bg(theme::selected()))
                                            .child("↓")
                                            .on_click(move |_, _, cx| {
                                                cx.stop_propagation();
                                                down.update(cx, |this, cx| {
                                                    this.move_answer(id, place, false, cx)
                                                })
                                                .ok();
                                            }),
                                    )
                            })),
                    )
                    .children((!o.code.is_empty()).then(|| {
                        div()
                            .ml(px(38.))
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .bg(theme::editor())
                            .font_family(theme::code_font())
                            .text_size(px(12.))
                            .children(o.code.iter().map(|l| div().child(l.clone())))
                    }))
                    .when(!order_kind, |s| {
                        s.on_click(move |_, _, cx| {
                            pick.update(cx, |this, cx| this.pick_answer(id, Some(i), cx))
                                .ok();
                        })
                    });
                // The option picked lights up (#56, 17).
                let tint = theme::agent_bg();
                Some(
                    super::motion::toward(
                        row,
                        ("form-option-on", id as usize * 64 + i),
                        on,
                        false,
                        super::motion::FAST,
                        move |row, v| {
                            row.bg(gpui::Rgba {
                                a: tint.a * v,
                                ..tint
                            })
                        },
                    )
                    .into_any_element(),
                )
            })
            .collect()
    }

    fn send_to_chat(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(pick) = self.agent_pick.take() else {
            return;
        };
        let Some(chat) = pick.chats.get(ix).cloned() else {
            return;
        };
        if let Some(branch) = pick.route {
            self.chats
                .update(cx, |c, _| c.routes.insert(branch, chat.session));
            return cx.notify();
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let title = if chat.session.is_empty() {
            "a new chat".to_owned()
        } else {
            chat.title.clone()
        };
        self.change_comments(cx, |list| {
            agents::send(list, &pick.ids, &chat.session, &title)
        });
        self.save_comments(cx);
        self.codex_beat(cx);
        if chat.session.is_empty() {
            let prompt = "Artoo sent review comments to this chat. Call the artoo list_comments tool, change the code for each comment and answer each with address_comment.";
            if let Err(e) = crate::platform::open_terminal(&root, "claude", prompt) {
                self.error =
                    Some(format!("Could not open a terminal for the new chat: {e}").into());
            }
        }
        self.refresh_rows(cx);
        cx.notify();
    }

    pub(super) fn agent_pick_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(pick) = self.agent_pick.as_mut() else {
            return;
        };
        let last = pick.chats.len().saturating_sub(1);
        match event.keystroke.key.as_str() {
            "escape" => self.agent_pick = None,
            "enter" => {
                let ix = pick.selected;
                return self.send_to_chat(ix, cx);
            }
            "down" | "j" | "tab" => pick.selected = (pick.selected + 1).min(last),
            "up" | "k" => pick.selected = pick.selected.saturating_sub(1),
            _ => return,
        }
        cx.notify();
    }

    /// Codex has no watcher of its own: its open sessions here listen through the app, and what
    /// was sent to one is handed over (`crate::codex`, #18) — off the window's thread, one beat
    /// at a time; a snapshot leaves Codex alone.
    fn codex_beat(&mut self, cx: &mut Context<Self>) {
        let snapshot = cfg!(feature = "snapshot") && std::env::var_os("ARTOO_SNAPSHOT").is_some();
        if snapshot || self.codex_beat.is_some() {
            return;
        }
        let (Some(project), Some(bus)) = (self.project.clone().or(self.root.clone()), Self::bus())
        else {
            return;
        };
        self.codex_beat = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .spawn(async move { crate::codex::beat(&bus, &project) })
                .await;
            this.update(cx, |this, _| this.codex_beat = None).ok();
        }));
    }

    /// Takes in what the agents answered since the last look (from the 2-second poll).
    pub(super) fn poll_agents(&mut self, cx: &mut Context<Self>) {
        // A row an agent called the reviewer to, once its page is on screen, has been seen (#160);
        // however it was reached, a row, a link, a search.
        if let Some(item) = self.nav_page().and_then(super::nav::Page::item) {
            self.agent_calls.remove(&item);
        }
        // The chats are read on every beat: the stripe shows who listens, a toast who comes and
        // goes (#77).
        let (root, here) = (
            self.project.clone().or(self.root.clone()),
            self.root.clone(),
        );
        let comments = self.comments(cx).to_vec();
        let (changed, said) = self
            .chats
            .update(cx, |c, cx| c.beat(root.as_deref(), here, &comments, cx));
        for text in said {
            self.toast_info(text.into(), cx);
        }
        if changed {
            cx.notify();
        }
        self.codex_beat(cx);
        let (Some(root), Some(bus)) = (self.project.clone().or(self.root.clone()), Self::bus())
        else {
            return;
        };
        // What agents left on requests, and what they ask to show (#150).
        self.agent_events(&root, &bus, cx);
        if let Some(before) = self.agent_comments.update(cx, |c, _| c.take_answers(&bus)) {
            self.refresh_rows(cx);
            self.notify_answers(&before, cx);
            cx.notify();
        }
    }

    /// Reads this repository's chats again (`Chats::refresh`); whether they changed.
    pub(super) fn refresh_chats(&mut self, cx: &mut Context<Self>) -> bool {
        let root = self.project.clone().or(self.root.clone());
        let comments = self.agent_comments.clone();
        self.chats.update(cx, |c, cx| {
            c.refresh(root.as_deref(), &comments.read(cx).list)
        })
    }

    /// A click in the Agents window (`artoo_agents::window::Act`).
    fn agents_act(&mut self, act: Act, cx: &mut Context<Self>) {
        match act {
            Act::Show(id) => self.jump_to(id, cx),
            Act::Send(ids) => self.send_agent(ids, cx),
            Act::PickChat => self.open_agent_pick(cx),
            Act::Route(branch) => self.open_route_pick(branch, cx),
            Act::Edit(id) => self.start_edit(super::composer::Change::Agent(id), cx),
            Act::Delete(id) => self.delete_draft(id, cx),
            Act::Resend(id) => self.resend(id, cx),
            Act::Withdraw(id) => self.withdraw(id, cx),
            Act::Archive(id, on) => self.set_archived(id, on, cx),
            Act::ArchiveResolved => self.archive_resolved(cx),
            Act::Resolve(id) => self.resolve_agent_comment(id, cx),
            Act::NameChat { session, agent } => {
                self.open_label(super::labeling::Target::Chat { session, agent }, cx)
            }
        }
    }

    /// A draft goes, no question asked; the toast's Undo brings it back (#76).
    pub(super) fn delete_draft(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(comment) = self.comments(cx).iter().find(|c| c.id == id).cloned() else {
            return;
        };
        self.delete_comment(id, cx);
        self.toast_deleted(comment, cx);
    }

    /// A sent one its chat has not taken yet comes back to the drafts.
    fn withdraw(&mut self, id: u64, cx: &mut Context<Self>) {
        let withdrawn = self.change_comment(id, cx, |c| {
            c.state = State::Draft;
            c.sent = None;
        });
        if withdrawn.is_some() {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    fn set_archived(&mut self, id: u64, on: bool, cx: &mut Context<Self>) {
        if self.change_comment(id, cx, |c| c.archived = on).is_some() {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    /// Archive resolved: every resolved one goes off the lists and its line.
    fn archive_resolved(&mut self, cx: &mut Context<Self>) {
        self.change_comments(cx, |list| {
            for c in list.iter_mut().filter(|c| c.state == State::Resolved) {
                c.archived = true;
            }
        });
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// The reviewer agrees the agent did it.
    pub(super) fn resolve_agent_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.set_agent_state(id, State::Resolved, cx);
    }

    /// Not done: back to a draft, to be sent again (with what was said so far).
    pub(super) fn reopen_agent_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.set_agent_state(id, State::Draft, cx);
    }

    fn set_agent_state(&mut self, id: u64, state: State, cx: &mut Context<Self>) {
        if self.change_comment(id, cx, |c| c.state = state).is_some() {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    pub(super) fn render_agent_pick(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let pick = self.agent_pick.as_ref()?;
        let count = pick.ids.len();
        let rows = pick.chats.iter().enumerate().map(|(ix, chat)| {
            let on = ix == pick.selected;
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
                        .child(super::hover::cut(
                            ("pick-chat-title", ix),
                            chat.title.clone(),
                        ))
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
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_to_chat(ix, cx)))
        });
        Some(
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
                        this.agent_pick = None;
                        cx.notify();
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
                                .child(match &pick.route {
                                    Some(b) if !b.is_empty() => format!("Notes on {b} go to"),
                                    Some(_) => "Notes go to".to_owned(),
                                    None => format!(
                                        "Send {count} comment{} to",
                                        if count == 1 { "" } else { "s" }
                                    ),
                                }),
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
                ),
        )
    }

    /// Settings → Agents: connect Claude Code (every profile), the commands for others.
    pub(super) fn render_agents_settings(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let server = agents::server_command();
        let claude_cmd = format!("claude mcp add -s user artoo -- {server}");
        let codex_cmd = crate::codex::add_command(&server);
        let profiles = agents::claude_profiles()
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(", ");
        let connect = div()
            .id("agents-connect-claude")
            .px_3()
            .h(px(28.))
            .flex()
            .items_center()
            .rounded(px(ROW_RADIUS))
            .bg(theme::agent())
            .text_color(theme::base())
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .child(if self.agent_setup_busy {
                "Connecting…"
            } else {
                "Connect"
            })
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.connect_claude(cx)));
        // `tip`: what the label leaves out, on a hover.
        let command =
            |id: &'static str, label: &'static str, tip: Option<&'static str>, text: String| {
                let copy = text.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .py(px(8.))
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("{id}-label")))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(label)
                            .when_some(tip, |d, tip| {
                                d.tooltip(move |_, cx| cx.new(|_| super::Tip(tip)).into())
                            }),
                    )
                    .child(
                        div()
                            .id(id)
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded(px(ROW_RADIUS))
                            .bg(theme::base())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover()))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_family(theme::code_font())
                                    .text_size(px(12.))
                                    .child(text),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child("Copy"),
                            )
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                            }),
                    )
            };
        div()
            .flex()
            .flex_col()
            .child(super::settings::item(
                "Claude Code",
                "Adds Artoo's MCP server to every Claude Code profile; a chat already open needs a restart.",
                connect,
            ))
            .children(self.agent_setup.clone().map(|text| {
                div()
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(text)
            }))
            .child(
                div()
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child(if profiles.is_empty() {
                        "No Claude Code profile found.".to_owned()
                    } else {
                        format!("Profiles: {profiles}")
                    }),
            )
            .child(command(
                "agents-cmd-claude",
                "Or by hand, in a terminal:",
                None,
                claude_cmd,
            ))
            .child(command(
                "agents-cmd-codex",
                "Codex (limited):",
                Some(crate::codex::LIMITS),
                codex_cmd,
            ))
    }

    pub(super) fn connect_claude(&mut self, cx: &mut Context<Self>) {
        if self.agent_setup_busy {
            return;
        }
        self.agent_setup_busy = true;
        self.agent_setup = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { agents::connect_claude() })
                .await;
            this.update(cx, |this, cx| {
                this.agent_setup_busy = false;
                this.setup_look_again();
                this.agent_setup = Some(match result {
                    Ok(text) => text,
                    Err(text) => format!("Not connected — {text}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// What the Agents window asks of the tab (`artoo_agents::window::AgentsHost`).
pub(super) struct AgentsHost(pub WeakEntity<Workspace>);

impl artoo_agents::window::AgentsHost for AgentsHost {
    fn branch(&self, cx: &App) -> Option<String> {
        self.0.upgrade()?.read(cx).page_branch(cx)
    }

    fn rich(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|tab| tab.read(cx).settings.rich_comments)
    }

    fn act(&self, act: Act, cx: &mut App) {
        self.0.update(cx, |this, cx| this.agents_act(act, cx)).ok();
    }
}
