//! A session's conversation as the Agents tab draws it (#220): the record (`artoo_agents_api::conversation`) grouped
//! the way a messenger groups — the agent's messages under one name, the reviewer's on the right, a day line, a status
//! line, `Claude is working on this` under what it is about. `blocks` is pure; `view` draws what it says.

use crate::review::{Answered, Comment, Form, FormKind};
use artoo_agents_api::conversation::{Entry, Ref};
use artoo_agents_api::view::{Act, AgentsHost, FormEvent, OnForm};
use artoo_ui_kit::{format, hover, icons, markdown, px, theme};
use gpui::{AnyElement, App, FontWeight, Rgba, SharedString, div, prelude::*};
use std::rc::Rc;

/// One thing the agent said in a group.
#[derive(Clone, Debug, PartialEq)]
pub struct Say {
    pub at: i64,
    pub text: String,
    pub on: Option<Ref>,
    pub form: Option<Form>,
    pub answered: Option<Answered>,
}

/// One message of the reviewer's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mine {
    Words { at: i64, text: String },
    Comment { at: i64, id: u64 },
}

/// A block of the conversation, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// `Today`, `Yesterday`, a date.
    Day(String),
    /// The agent's messages in a row, under one name.
    Claude { at: i64, says: Vec<Say> },
    /// The reviewer's messages in a row.
    You { mine: Vec<Mine> },
    /// A centred line: `Done · 3 files changed`.
    Status { at: i64, text: String },
    /// `Claude is working on this` / `on these N`, under the comments it is about.
    Working { these: usize },
    /// At the end, once others stand between: `the other N of your comments`; `answered` the last one that has
    /// its answer, whose place opens the line.
    Tail {
        others: usize,
        answered: Option<u64>,
    },
}

/// How a day reads: today and yesterday by name, else the date.
fn day_of(time: i64, now: i64) -> String {
    let day = format::date(time);
    if day == format::date(now) {
        "Today".to_owned()
    } else if day == format::date(now - 86_400) {
        "Yesterday".to_owned()
    } else {
        day
    }
}

/// The conversation's blocks. `working` are the comments the agent has not answered (`working_on`).
pub fn blocks(talk: &[Entry], comments: &[Comment], working: &[u64], now: i64) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let mut day = String::new();
    for entry in talk {
        let this = format::date(entry.at());
        if this != day {
            day = this;
            out.push(Block::Day(day_of(entry.at(), now)));
        }
        match entry {
            Entry::You { at, text, .. } => {
                let mine = Mine::Words {
                    at: *at,
                    text: text.clone(),
                };
                match out.last_mut() {
                    Some(Block::You { mine: all }) => all.push(mine),
                    _ => out.push(Block::You { mine: vec![mine] }),
                }
            }
            Entry::Comment { at, id } => {
                let mine = Mine::Comment { at: *at, id: *id };
                match out.last_mut() {
                    Some(Block::You { mine: all }) => all.push(mine),
                    _ => out.push(Block::You { mine: vec![mine] }),
                }
            }
            Entry::Claude {
                at,
                text,
                on,
                form,
                answered,
            } => {
                let say = Say {
                    at: *at,
                    text: text.clone(),
                    on: *on,
                    form: form.clone(),
                    answered: answered.clone(),
                };
                match out.last_mut() {
                    Some(Block::Claude { says, .. }) => says.push(say),
                    _ => out.push(Block::Claude {
                        at: *at,
                        says: vec![say],
                    }),
                }
            }
            Entry::Status { at, text } => out.push(Block::Status {
                at: *at,
                text: text.clone(),
            }),
        }
    }
    working_lines(&mut out, comments, working);
    out
}

/// The working line under each group with comments the agent has not answered, and the end line when something
/// stands between the last of them and the end.
fn working_lines(out: &mut Vec<Block>, comments: &[Comment], working: &[u64]) {
    if working.is_empty() {
        return;
    }
    let mut at = 0;
    let mut last_line = None;
    while at < out.len() {
        if let Block::You { mine } = &out[at] {
            let these = mine
                .iter()
                .filter(|m| matches!(m, Mine::Comment { id, .. } if working.contains(id)))
                .count();
            if these > 0 {
                out.insert(at + 1, Block::Working { these });
                last_line = Some(at + 1);
                at += 1;
            }
        }
        at += 1;
    }
    if last_line.is_some_and(|i| i + 1 < out.len()) {
        // The answered comment the reviewer last heard of, to name the place the line is about.
        let answered = out.iter().rev().find_map(|b| match b {
            Block::Claude { says, .. } => says.iter().rev().find_map(|s| match s.on {
                Some(Ref::Comment(id) | Ref::Plan(id) | Ref::Question(id))
                    if comments.iter().any(|c| c.id == id) =>
                {
                    Some(id)
                }
                _ => None,
            }),
            _ => None,
        });
        out.push(Block::Tail {
            others: working.len(),
            answered,
        });
    }
}

/// What the conversation is drawn with that the tab knows.
pub struct Look {
    /// The agent's name, over its messages.
    pub name: String,
    /// Its mark.
    pub mark: (char, Rgba),
    /// Comments and messages drawn as markdown.
    pub rich: bool,
    pub host: Option<Rc<dyn AgentsHost>>,
}

/// A comment's place, short: `LoginViewModel.kt:51`.
fn place(c: &Comment) -> String {
    c.reads()
}

fn clock(at: i64) -> String {
    format::clock(at)
}

/// The conversation's blocks drawn, in a column.
pub fn view(blocks: &[Block], comments: &[Comment], look: &Look, cx: &App) -> Vec<AnyElement> {
    blocks
        .iter()
        .enumerate()
        .map(|(ix, b)| match b {
            Block::Day(label) => day(label),
            Block::Claude { at, says } => claude(ix, *at, says, comments, look, cx),
            Block::You { mine } => you(ix, mine, comments, look, cx),
            Block::Status { text, .. } => status(text),
            Block::Working { these } => working(*these, look),
            Block::Tail { others, answered } => tail(*others, *answered, comments, look),
        })
        .collect()
}

fn day(label: &str) -> AnyElement {
    div()
        .w_full()
        .flex()
        .justify_center()
        .py(px(2.))
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(label.to_owned())
        .into_any_element()
}

fn status(text: &str) -> AnyElement {
    let (what, count) = text.split_once(" · ").unwrap_or((text, ""));
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            icons::icon("check")
                .size(px(14.))
                .text_color(theme::faint()),
        )
        .child(what.to_owned())
        .children(
            (!count.is_empty()).then(|| div().text_color(theme::faint()).child(count.to_owned())),
        )
        .into_any_element()
}

/// `Claude is working on this`, under what it is about.
fn working(these: usize, look: &Look) -> AnyElement {
    let words = if these == 1 {
        "Claude is working on this".to_owned()
    } else {
        format!("Claude is working on these {these}")
    };
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            crate::view::chat_badge(look.mark.0, look.mark.1, false)
                .size(px(20.))
                .text_size(px(10.)),
        )
        .child(dots())
        .child(words)
        .into_any_element()
}

/// Three quiet dots: it is on it.
fn dots() -> gpui::Div {
    div()
        .flex()
        .gap(px(3.))
        .children((0..3).map(|_| div().size(px(5.)).rounded_full().bg(theme::faint())))
}

fn tail(others: usize, answered: Option<u64>, comments: &[Comment], look: &Look) -> AnyElement {
    let words = format!("Claude is working on the other {others} of your comments");
    let place_of = answered
        .and_then(|id| comments.iter().find(|c| c.id == id))
        .map(|c| (c.id, place(c)));
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .children(place_of.map(|(id, at)| reply_block(id, at, look)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(
                    crate::view::chat_badge(look.mark.0, look.mark.1, false)
                        .size(px(20.))
                        .text_size(px(10.)),
                )
                .child(dots())
                .child(words),
        )
        .into_any_element()
}

/// The one block that opens an answer: the reply glyph and what it answers; a click shows the comment on its line.
fn reply_block(id: u64, what: String, look: &Look) -> AnyElement {
    let host = look.host.clone();
    div()
        .id(("talk-reply", id as usize))
        .self_start()
        .max_w_full()
        .flex()
        .items_center()
        .gap(px(6.))
        .mb(px(6.))
        .px(px(8.))
        .py(px(3.))
        .rounded(px(4.))
        .border_l_2()
        .border_color(theme::accent())
        .bg(theme::editor())
        .text_size(px(11.))
        .text_color(theme::muted())
        .cursor_pointer()
        .hover(|d| d.bg(theme::base()))
        .tooltip_show_delay(hover::LONG)
        .tooltip(|_, cx| {
            cx.new(|_| hover::Tip("Show the comment, on its line"))
                .into()
        })
        .child(icons::icon("reply").size(px(12.)))
        .child(hover::cut(("talk-reply-what", id as usize), what))
        .on_click(move |_, _, cx| {
            if let Some(host) = &host {
                host.act(Act::Show(id), cx);
            }
        })
        .into_any_element()
}

/// The reviewer's messages: right, one bubble each, a comment opening with its place alone.
fn you(ix: usize, mine: &[Mine], comments: &[Comment], look: &Look, cx: &App) -> AnyElement {
    let bubbles = mine.iter().enumerate().map(|(n, m)| {
        let key: SharedString = format!("you-{ix}-{n}").into();
        let (head, text, at) = match m {
            Mine::Words { at, text } => (None, text.clone(), *at),
            Mine::Comment { at, id } => match comments.iter().find(|c| c.id == *id) {
                Some(c) => (Some((c.id, place(c))), c.body.clone(), *at),
                None => (None, String::new(), *at),
            },
        };
        let host = look.host.clone();
        div()
            .max_w(px(300.))
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .px(px(10.))
            .py(px(7.))
            .rounded_tl(px(12.))
            .rounded_bl(px(12.))
            .rounded_br(px(12.))
            .rounded_tr(px(6.))
            .bg(theme::selected())
            .text_size(px(13.))
            .children(head.map(|(id, what)| {
                div()
                    .id(("talk-src", id as usize))
                    .self_start()
                    .max_w_full()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(theme::hover())
                    .text_size(px(11.))
                    .text_color(theme::muted())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::text()))
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| {
                        cx.new(|_| hover::Tip("Show it on its line, where you wrote it"))
                            .into()
                    })
                    .child(icons::icon("file").size(px(12.)))
                    .child(hover::cut(("talk-src-what", id as usize), what))
                    .child(icons::icon("external").size(px(11.)))
                    .on_click(move |_, _, cx| {
                        if let Some(host) = &host {
                            host.act(Act::Show(id), cx);
                        }
                    })
            }))
            .child(markdown::body(&text, look.rich, key, None, None, cx))
            .child(
                div()
                    .self_end()
                    .text_size(px(10.))
                    .text_color(theme::faint())
                    .child(clock(at)),
            )
    });
    // 12 between comments sent together; 20 apart from the next block (the column's gap).
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_end()
        .gap(px(12.))
        .children(bubbles)
        .into_any_element()
}

/// The agent's messages under one name: its mark, `Claude` and the time, then a bubble each.
fn claude(
    ix: usize,
    at: i64,
    says: &[Say],
    comments: &[Comment],
    look: &Look,
    cx: &App,
) -> AnyElement {
    let bubbles = says
        .iter()
        .enumerate()
        .map(|(n, s)| bubble(ix * 1000 + n, s, comments, look, cx));
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(8.))
        .child(
            crate::view::chat_badge(look.mark.0, look.mark.1, false)
                .size(px(24.))
                .text_size(px(12.)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .text_size(px(12.))
                        .child(
                            div()
                                .flex_1()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(look.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(clock(at)),
                        ),
                )
                .children(bubbles),
        )
        .into_any_element()
}

/// Whether `s` is the form of `c`'s last question, still open for an answer.
fn open_form<'a>(s: &Say, comments: &'a [Comment]) -> Option<&'a Comment> {
    let id = match s.on? {
        Ref::Plan(id) | Ref::Question(id) => id,
        Ref::Comment(_) => return None,
    };
    let c = comments.iter().find(|c| c.id == id)?;
    let last = c.replies.last()?;
    (c.open() && last.form.is_some() && last.form == s.form && last.at == s.at).then_some(c)
}

/// What a question was answered with, for its quiet line.
fn did(s: &Say) -> String {
    let (Some(form), Some(a)) = (&s.form, &s.answered) else {
        return String::new();
    };
    if form.kind == FormKind::Plan {
        return "You approved it".to_owned();
    }
    let gist = a.gist(Some(form));
    if gist.is_empty() {
        String::new()
    } else {
        format!("You chose “{gist}”")
    }
}

fn bubble(key: usize, s: &Say, comments: &[Comment], look: &Look, cx: &App) -> AnyElement {
    let corners = |d: gpui::Div| {
        d.rounded_tl(px(6.))
            .rounded_tr(px(12.))
            .rounded_br(px(12.))
            .rounded_bl(px(12.))
    };
    let reply = s.on.and_then(|on| match on {
        Ref::Comment(id) => comments
            .iter()
            .find(|c| c.id == id)
            .map(|c| reply_block(id, place(c), look)),
        Ref::Plan(id) => Some(reply_block(
            id,
            format!(
                "Plan  {}",
                format::plural(s.form.as_ref().map_or(0, |f| f.options.len()), "step")
            ),
            look,
        )),
        Ref::Question(id) => comments.iter().find(|c| c.id == id).and_then(|c| {
            let form = s.form.as_ref();
            let words = c
                .answer
                .as_ref()
                .map(|a| a.gist(form))
                .filter(|g| !g.is_empty())?;
            Some(reply_block(id, format!("You chose “{words}”"), look))
        }),
    });
    let words = markdown::body(&s.text, true, format!("say-{key}"), None, None, cx);
    let card = match open_form(s, comments) {
        // Plan and question open: an orange border, the form under the words.
        Some(c) => {
            let host = look.host.clone();
            let on: OnForm = Rc::new(move |event: FormEvent, cx: &mut App| {
                if let Some(host) = &host {
                    host.act(Act::Form(event), cx);
                }
            });
            let steps = crate::form::bare(c, None, on);
            corners(div())
                .border_1()
                .border_color(theme::warning())
                .p(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .overflow_hidden()
                .text_size(px(13.))
                .children(reply)
                .child(words)
                .children(steps)
        }
        None if s.form.is_some() => {
            // Answered, or gone: one quiet line.
            let (glyph, what) = match s.form.as_ref().map(|f| f.kind) {
                Some(FormKind::Plan) => ("list", "Plan".to_owned()),
                _ => ("bot", s.text.clone()),
            };
            let steps = s
                .form
                .as_ref()
                .filter(|f| f.kind == FormKind::Plan)
                .map(|f| format::plural(f.options.len(), "step"));
            corners(div())
                .bg(theme::hover())
                .px(px(10.))
                .py(px(7.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .text_size(px(13.))
                // A plan or a question answered is one quiet line: what it was is in it.
                .children(reply.filter(|_| !matches!(s.on, Some(Ref::Plan(_)))))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .child(icons::icon(glyph).size(px(14.)).text_color(theme::muted()))
                        .child(div().flex_1().min_w_0().child(hover::cut(
                            SharedString::from(format!("talk-q-{key}")),
                            what,
                        )))
                        .children(steps.map(|n| div().text_color(theme::muted()).child(n)))
                        .child(div().text_color(theme::muted()).child(did(s))),
                )
        }
        None => corners(div())
            .bg(theme::hover())
            .px(px(10.))
            .py(px(7.))
            .flex()
            .flex_col()
            .text_size(px(13.))
            .children(reply)
            .child(words),
    };
    card.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_agents_api::comment::{Sent, State, send};

    const NOW: i64 = 1_800_000_000;

    fn comment(id: u64) -> Comment {
        Comment {
            id,
            path: "a.rs".into(),
            body: format!("Comment {id}"),
            ..Default::default()
        }
    }

    fn comment_entry(id: u64, at: i64) -> Entry {
        Entry::Comment { at, id }
    }

    fn says(at: i64, on: Option<Ref>) -> Entry {
        Entry::Claude {
            at,
            text: "ok".into(),
            on,
            form: None,
            answered: None,
        }
    }

    #[test]
    fn comments_sent_together_are_one_group_and_the_agents_words_another() {
        let talk = [
            Entry::You {
                at: NOW,
                text: "In order.".into(),
                carried: vec![1, 2],
            },
            comment_entry(1, NOW),
            comment_entry(2, NOW),
            says(NOW + 5, Some(Ref::Comment(1))),
            says(NOW + 6, None),
            Entry::Status {
                at: NOW + 7,
                text: "Done · 2 files changed".into(),
            },
        ];
        let all = blocks(&talk, &[comment(1), comment(2)], &[], NOW);
        assert!(matches!(&all[0], Block::Day(d) if d == "Today"));
        assert!(matches!(&all[1], Block::You { mine } if mine.len() == 3));
        assert!(matches!(&all[2], Block::Claude { says, .. } if says.len() == 2));
        assert!(matches!(&all[3], Block::Status { .. }));
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn a_new_day_gets_a_line() {
        let talk = [
            comment_entry(1, NOW - 2 * 86_400),
            comment_entry(2, NOW - 86_400),
            comment_entry(3, NOW),
        ];
        let all = blocks(&talk, &[], &[], NOW);
        let days: Vec<&str> = all
            .iter()
            .filter_map(|b| match b {
                Block::Day(d) => Some(d.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(days.len(), 3);
        assert_eq!(&days[1..], ["Yesterday", "Today"]);
    }

    #[test]
    fn the_working_line_sits_under_its_comments_and_counts_them() {
        let mut all: Vec<Comment> = (1..=10).map(comment).collect();
        let ids: Vec<u64> = (1..=10).collect();
        send(&mut all, &ids, "s1", "t");
        let talk: Vec<Entry> = ids.iter().map(|&id| comment_entry(id, NOW)).collect();
        let b = blocks(&talk, &all, &ids, NOW);
        assert!(matches!(b.last(), Some(Block::Working { these: 10 })));
        // One comment: `this`.
        let one = blocks(&talk[..1], &all, &ids[..1], NOW);
        assert!(matches!(one.last(), Some(Block::Working { these: 1 })));
        // Nothing unanswered: no line.
        assert!(
            !blocks(&talk, &all, &[], NOW)
                .iter()
                .any(|b| matches!(b, Block::Working { .. }))
        );
    }

    #[test]
    fn the_end_line_comes_once_others_stand_between() {
        let mut all: Vec<Comment> = (1..=3).map(comment).collect();
        send(&mut all, &[1, 2, 3], "s1", "t");
        all[0].sent = Some(Sent {
            session: "s1".into(),
            at: NOW,
            ..Default::default()
        });
        all[0].state = State::Sent;
        let talk = [
            comment_entry(1, NOW),
            comment_entry(2, NOW),
            comment_entry(3, NOW),
            says(NOW + 9, Some(Ref::Comment(1))),
        ];
        let b = blocks(&talk, &all, &[2, 3], NOW);
        assert!(matches!(
            b.last(),
            Some(Block::Tail {
                others: 2,
                answered: Some(1)
            })
        ));
        // The working line under the group still says how many it is about.
        assert!(b.iter().any(|x| matches!(x, Block::Working { these: 2 })));
    }
}
