//! A session's discussions (the owner, 2026-11-27: no chat): each comment written for the agent is a discussion of its
//! own, drawn as a request's are in its Discussions window — your words, then each thing the agent said on them, a
//! question with its form open where it waits for you. What needs you comes first, the newest on top (a question it
//! asks, a comment it says it did); then your drafts, which Send all at the foot sends; then what the agent has; the
//! done ones folded at the end. Pure grouping (`grouped`) and the drawing of it (`view`).

use crate::agents::Chat;
use crate::review::{Comment, FormKind, State};
use artoo_agents_api::comment::{Kind, kind_of};
use artoo_agents_api::view::{Act, AgentsHost, FormEvent, OnForm};
use artoo_ui_kit::controls::edge;
use artoo_ui_kit::discussion::{self, Kind as Button};
use artoo_ui_kit::hover::{self, Hover as _};
use artoo_ui_kit::{ROW_RADIUS, format, icons, markdown, monogram, px, theme};
use gpui::{AnyElement, App, ClickEvent, FontWeight, Rgba, SharedString, div, prelude::*};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

/// A group of the session's discussions, in the order they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Group {
    /// The agent asked something, or says it did what you asked: yours to answer or to close.
    NeedsYou,
    /// Written, not sent: what Send all sends.
    Drafts,
    /// Sent: the agent has it, or its chat has not taken it yet.
    WithAgent,
    /// Closed by you.
    Done,
}

impl Group {
    pub fn word(self) -> &'static str {
        match self {
            Group::NeedsYou => "Needs you",
            Group::Drafts => "Drafts",
            Group::WithAgent => "Working on",
            Group::Done => "Done",
        }
    }
}

/// When a comment last moved: the agent's last word on it, else when it was sent, else written.
fn last_moved(c: &Comment) -> i64 {
    let sent = c.sent.as_ref().map_or(0, |s| s.at);
    c.replies
        .iter()
        .map(|r| r.at)
        .chain([sent, c.written])
        .max()
        .unwrap_or(0)
}

/// The session's comments by group, each group in the order it is drawn, the empty ones left out: what needs you the
/// newest first, the drafts as written, what the agent has the newest first, the done ones the newest first. A sent
/// comment is the session's when it went to it, or to no chat in particular; a draft when it goes to it (its branch).
pub fn grouped<'a>(
    comments: &'a [Comment],
    chat: &Chat,
    alive: &dyn Fn(&str) -> bool,
) -> Vec<(Group, Vec<&'a Comment>)> {
    let ours = |c: &&Comment| {
        !c.archived
            && c.sent
                .as_ref()
                .is_some_and(|s| s.session.is_empty() || s.session == chat.session)
    };
    let mut needs: Vec<&Comment> = Vec::new();
    let mut with: Vec<&Comment> = Vec::new();
    let mut done: Vec<&Comment> = Vec::new();
    for c in comments.iter().filter(ours) {
        match kind_of(c, alive) {
            Kind::Asks | Kind::Addressed => needs.push(c),
            Kind::Sent | Kind::Working | Kind::Waiting => with.push(c),
            Kind::Resolved => done.push(c),
            Kind::Draft => {}
        }
    }
    for list in [&mut needs, &mut with, &mut done] {
        list.sort_by_key(|c| std::cmp::Reverse(last_moved(c)));
    }
    let drafts = crate::sessions::drafts_of(comments, chat);
    [
        (Group::NeedsYou, needs),
        (Group::Drafts, drafts),
        (Group::WithAgent, with),
        (Group::Done, done),
    ]
    .into_iter()
    .filter(|(_, list)| !list.is_empty())
    .collect()
}

/// What the discussions are drawn with that the window knows.
pub struct Look {
    /// The agent's name (`Claude Code`) and its mark.
    pub agent: String,
    pub mark: (char, Rgba),
    pub rich: bool,
    pub host: Option<Rc<dyn AgentsHost>>,
    /// The session's folder, where a `path:line` in the agent's words is looked for.
    pub folder: PathBuf,
    pub session: String,
    pub title: String,
    /// The groups folded (Done starts folded: the window keeps which are flipped).
    pub folded: HashSet<Group>,
    /// A group's heading clicked.
    pub fold: OnGroup,
    /// What the window does itself to a comment: closed, opened again, taken back.
    pub own: OnOwn,
}

/// What a group's heading does when clicked.
pub type OnGroup = Rc<dyn Fn(Group, &mut App)>;
/// What the window does to a comment itself.
pub type OnOwn = Rc<dyn Fn(Own, &mut App)>;

/// What the window does to a comment itself, not the tab.
#[derive(Clone, Copy, Debug)]
pub enum Own {
    /// You agree it is done.
    Close(u64),
    /// Not done: a draft again, its thread kept, to be sent again.
    Reopen(u64),
    /// Sent and not taken yet: a draft again.
    TakeBack(u64),
}

impl Look {
    /// The agent's short name: `Claude`.
    fn short(&self) -> &str {
        self.agent.trim_end_matches(" Code")
    }

    fn act(&self, act: Act, cx: &mut App) {
        if let Some(host) = &self.host {
            host.act(act, cx);
        }
    }
}

/// The groups drawn: each a heading that folds it (its chevron at the row's far end) over its cards.
pub fn view(groups: &[(Group, Vec<&Comment>)], look: &Rc<Look>, cx: &App) -> Vec<AnyElement> {
    let mut out = Vec::new();
    for (group, list) in groups {
        let open = !look.folded.contains(group);
        out.push(heading(*group, list.len(), open, look));
        if open {
            out.extend(list.iter().map(|c| card(c, *group, look, cx)));
        }
    }
    out
}

/// A group's heading: its word and count, the chevron at the far end; the whole row folds it, lit under the pointer.
fn heading(group: Group, count: usize, open: bool, look: &Rc<Look>) -> AnyElement {
    let fold = look.fold.clone();
    let tint = match group {
        Group::NeedsYou => theme::warning(),
        _ => theme::muted(),
    };
    div()
        .id(SharedString::from(format!("agents-group-{}", group.word())))
        .mx(px(8.))
        .mt(px(6.))
        .mb(px(4.))
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .hover(|d| d.bg(theme::hover()))
        .child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tint)
                .child(group.word()),
        )
        .child(
            div()
                .min_w(px(18.))
                .h(px(18.))
                .px(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(monogram::tint(tint))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tint)
                .child(count.to_string()),
        )
        .child(div().flex_1())
        .child(
            icons::icon(if open {
                "chevron-down"
            } else {
                "chevron-right"
            })
            .size(px(14.))
            .text_color(theme::muted()),
        )
        .tip(if open {
            format!("Fold {}", group.word())
        } else {
            format!("Show {}", group.word())
        })
        .on_click(move |_: &ClickEvent, _, cx| fold(group, cx))
        .into_any_element()
}

/// A note of the discussion: the face, who and when on a line, the words under it.
fn note(face: gpui::Div, who: String, when: i64, body: AnyElement) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(face)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(who),
                )
                .children((when > 0).then(|| {
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(format::short_ago(when))
                })),
        )
        .child(div().pl(px(26.)).min_w_0().text_size(px(13.)).child(body))
}

/// The agent's face: its chat's letter on its tint.
fn agent_face(look: &Look) -> gpui::Div {
    crate::view::chat_badge(look.mark.0, look.mark.1, false)
}

/// A quiet line under an earlier question: what it was answered with.
fn answered_line(
    form: &crate::review::Form,
    answered: Option<&crate::review::Answered>,
) -> gpui::Div {
    let said = match (form.kind, answered) {
        (FormKind::Plan, Some(_)) => "You approved the plan".to_owned(),
        (_, Some(a)) => match a.gist(Some(form)) {
            g if g.is_empty() => "You answered".to_owned(),
            g => format!("You chose “{g}”"),
        },
        (_, None) => "Not answered — it asked again".to_owned(),
    };
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            icons::icon(if answered.is_some() {
                "circle-check"
            } else {
                "circle"
            })
            .size(px(12.))
            .text_color(theme::muted()),
        )
        .child(said)
}

/// One discussion: where it is, your words, each thing the agent said on them (the last question's form open while it
/// waits), your answer not sent yet, and the actions of its group at its foot.
fn card(c: &Comment, group: Group, look: &Rc<Look>, cx: &App) -> AnyElement {
    let id = c.id;
    let request = crate::talk::request_of(c, look.host.as_ref(), cx);
    let place = crate::talk::place(c, request);
    let open_question = matches!(c.state, State::Sent | State::Draft)
        && c.answer.is_none()
        && c.replies.last().is_some_and(|r| r.form.is_some());
    let last = c.replies.len().saturating_sub(1);
    let mut notes: Vec<gpui::Div> = vec![note(
        discussion::avatar("You", true, 18.),
        "You".to_owned(),
        c.written,
        markdown::body(
            &c.body,
            look.rich,
            format!("agent-thread-{id}"),
            None,
            None,
            cx,
        ),
    )];
    for (n, r) in c.replies.iter().enumerate() {
        let words = crate::talk::words(
            id as usize * 1000 + n,
            &r.body,
            &look.folder,
            look.host.clone(),
            cx,
        );
        let mut body = div().flex().flex_col().gap(px(8.)).child(words);
        match &r.form {
            Some(form) if n == last && open_question => {
                body = body.children(if form.kind == FormKind::Plan {
                    Some(crate::talk::plan_of(c, look.host.clone()))
                } else {
                    let host = look.host.clone();
                    let on: OnForm = Rc::new(move |event: FormEvent, cx: &mut App| {
                        if let Some(host) = &host {
                            host.act(Act::Form(event), cx);
                        }
                    });
                    crate::form::bare(c, None, Some(look.short()), on)
                });
            }
            Some(form) => {
                let answered = if n == last {
                    c.answer.as_ref().or(r.answered.as_ref())
                } else {
                    r.answered.as_ref()
                };
                body = body.child(answered_line(form, answered));
            }
            None => {}
        }
        notes.push(note(
            agent_face(look),
            look.short().to_owned(),
            r.at,
            body.into_any_element(),
        ));
    }
    // An answer given, waiting to go with the next send.
    let answer = (c.state == State::Draft)
        .then_some(c.answer.as_ref())
        .flatten()
        .map(|a| {
            let form = c.replies.last().and_then(|r| r.form.as_ref());
            let gist = a.gist(form);
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(26.))
                .text_size(px(12.))
                .text_color(theme::warning())
                .child(
                    icons::icon("pencil")
                        .size(px(12.))
                        .text_color(theme::warning()),
                )
                .child(if gist.is_empty() {
                    "Your answer, not sent yet".to_owned()
                } else {
                    format!("Your answer, not sent yet: {gist}")
                })
        });
    let rail = match group {
        Group::Drafts => Some(theme::warning()),
        Group::NeedsYou => Some(look.mark.1),
        _ => None,
    };
    let show = look.clone();
    div()
        .id(("agent-thread", id as usize))
        .mx(px(8.))
        .mb(px(8.))
        .px(px(10.))
        .py(px(8.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::editor())
        .relative()
        .when(group == Group::Done, |d| d.opacity(0.75))
        .children(rail.map(|r| edge(r, 2., 8.)))
        .child(
            // Where it is, the whole row a way to its line.
            div()
                .id(("agent-thread-place", id as usize))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .text_color(theme::faint())
                .hover(|d| d.text_color(theme::text()))
                .child(
                    icons::icon(if c.thread.is_some() || request.is_some() {
                        "pull-request"
                    } else {
                        "file"
                    })
                    .flex_none()
                    .size(px(12.))
                    .text_color(theme::faint()),
                )
                .child(
                    hover::cut(("agent-thread-where", id as usize), place)
                        .flex_1()
                        .min_w_0()
                        .font_family(theme::code_font())
                        .text_size(px(11.)),
                )
                .tip("Show it on its line")
                .on_click(move |_: &ClickEvent, _, cx| show.act(Act::Show(id), cx)),
        )
        .children(notes)
        .children(answer)
        .children(foot(c, group, look))
        .into_any_element()
}

/// A small framed button of a card's foot: a glyph and its words.
fn small(
    id: (&'static str, u64),
    glyph: &'static str,
    label: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id((id.0, id.1 as usize))
        .h(px(26.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .text_size(px(12.))
        .cursor_pointer()
        .hover(|d| d.bg(theme::hover()))
        .child(icons::icon(glyph).size(px(13.)).text_color(theme::muted()))
        .child(label)
}

/// A glyph alone, its words in its tip.
fn glyph(
    id: (&'static str, u64),
    name: &'static str,
    tip: &'static str,
    danger: bool,
) -> gpui::Stateful<gpui::Div> {
    let ink = if danger {
        theme::removed()
    } else {
        theme::muted()
    };
    div()
        .id((id.0, id.1 as usize))
        .size(px(26.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .when(danger, |d| d.bg(theme::danger_bg()))
        .hover(|d| d.bg(theme::hover()))
        .child(icons::icon(name).size(px(14.)).text_color(ink))
        .tip(tip)
}

/// A card's foot by its group: a draft's Edit, Delete and a send of its own; Close or Reopen what the agent did; take
/// back what it has not taken, send again what a gone chat never took; what the agent is at, said.
fn foot(c: &Comment, group: Group, look: &Rc<Look>) -> Option<AnyElement> {
    let id = c.id;
    let row = || div().flex().items_center().gap(px(6.));
    let alive = c.sent.as_ref().is_some_and(|s| s.delivered);
    match group {
        Group::Drafts => {
            let (edit, delete, send) = (look.clone(), look.clone(), look.clone());
            Some(
                row()
                    .child(div().flex_1())
                    .child(
                        glyph(("agent-thread-edit", id), "pencil", "Edit", false)
                            .on_click(move |_: &ClickEvent, _, cx| edit.act(Act::Edit(id), cx)),
                    )
                    .child(
                        glyph(("agent-thread-delete", id), "trash", "Delete", true)
                            .on_click(move |_: &ClickEvent, _, cx| delete.act(Act::Delete(id), cx)),
                    )
                    .child(small(("agent-thread-send", id), "send", "Send").on_click(
                        move |_: &ClickEvent, _, cx| {
                            send.act(
                                Act::SendTo {
                                    ids: vec![id],
                                    session: send.session.clone(),
                                    title: send.title.clone(),
                                },
                                cx,
                            )
                        },
                    ))
                    .into_any_element(),
            )
        }
        Group::NeedsYou if c.state == State::Addressed => {
            let (reopen, close) = (look.own.clone(), look.own.clone());
            Some(
                row()
                    .child(div().flex_1())
                    .child(
                        small(("agent-thread-reopen", id), "rotate-ccw", "Reopen")
                            .tip("Not done: a draft again, to say what is missing and send")
                            .on_click(move |_: &ClickEvent, _, cx| reopen(Own::Reopen(id), cx)),
                    )
                    .child(
                        discussion::button(
                            ("agent-thread-close", id as usize),
                            "Close",
                            None,
                            Button::Primary(look.mark.1),
                        )
                        .h(px(26.))
                        .px(px(10.))
                        .tip("It is done: close it")
                        .on_click(move |_: &ClickEvent, _, cx| close(Own::Close(id), cx)),
                    )
                    .into_any_element(),
            )
        }
        Group::WithAgent => {
            let gone = !alive
                && c.sent
                    .as_ref()
                    .is_some_and(|s| !s.session.is_empty() && s.session != look.session);
            let said = if alive {
                format!("{} is working on it", look.short())
            } else {
                format!("Not taken by {} yet", look.short())
            };
            let (back, again) = (look.own.clone(), look.clone());
            Some(
                row()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(said),
                    )
                    .children((!alive).then(|| {
                        small(("agent-thread-back", id), "undo", "Take back")
                            .tip("Back to your drafts")
                            .on_click(move |_: &ClickEvent, _, cx| back(Own::TakeBack(id), cx))
                    }))
                    .children(gone.then(|| {
                        small(("agent-thread-again", id), "send", "Send again")
                            .on_click(move |_: &ClickEvent, _, cx| again.act(Act::Resend(id), cx))
                    }))
                    .into_any_element(),
            )
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::{Form, Reply};
    use artoo_agents_api::comment::Sent;

    fn chat() -> Chat {
        Chat {
            session: "s1".to_owned(),
            ..Default::default()
        }
    }

    fn sent(id: u64, at: i64) -> Comment {
        Comment {
            id,
            state: State::Sent,
            written: at - 10,
            sent: Some(Sent {
                session: "s1".to_owned(),
                delivered: true,
                at,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn asked(mut c: Comment, at: i64) -> Comment {
        c.replies.push(Reply {
            uid: format!("r{}", c.id),
            body: "Which?".to_owned(),
            form: Some(Form::default()),
            at,
            answered: None,
        });
        c
    }

    #[test]
    fn what_needs_you_comes_first_the_latest_answer_on_top() {
        let mut done = sent(4, 50);
        done.state = State::Addressed;
        let comments = vec![
            asked(sent(1, 10), 20),
            sent(2, 30),
            asked(sent(3, 40), 90),
            done,
            Comment {
                id: 5,
                ..Default::default()
            },
        ];
        let groups = grouped(&comments, &chat(), &|_| true);
        let order: Vec<(Group, Vec<u64>)> = groups
            .iter()
            .map(|(g, l)| (*g, l.iter().map(|c| c.id).collect()))
            .collect();
        assert_eq!(
            order,
            vec![
                (Group::NeedsYou, vec![3, 4, 1]),
                (Group::Drafts, vec![5]),
                (Group::WithAgent, vec![2]),
            ]
        );
    }

    #[test]
    fn another_sessions_comments_are_not_listed() {
        let mut other = sent(1, 10);
        other.sent.as_mut().unwrap().session = "s2".to_owned();
        let all = [other];
        let groups = grouped(&all, &chat(), &|_| true);
        assert!(groups.is_empty());
    }
}
