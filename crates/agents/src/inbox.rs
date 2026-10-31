//! Your notes an agent answered, in the Inbox (#116): purple where a notice is blue. A row says who and what, the
//! note's place and the start of what the agent wrote; the answer selected shows whose and where, the lines the note
//! is on as they were, its thread (the tab's) and the way to its line. A click asks the tab an `AnswerEvent`.

use crate::review::{Comment, State};
use artoo_core::git::LineKind;
use artoo_core::markdown::plain;
use artoo_ui_kit::controls::bar_button;
use artoo_ui_kit::excerpt::{Excerpt, ExcerptLine, excerpt_view};
use artoo_ui_kit::hover::{self, CutInTip, Hover};
use artoo_ui_kit::{ROW_RADIUS, format, icons, px, theme};
use gpui::{AnyElement, App, ClickEvent, Div, FontWeight, Stateful, div, prelude::*};
use std::rc::Rc;

/// What a click on an agent's answer asks the tab.
pub enum AnswerEvent {
    /// Note `0`'s row clicked: selected, or opened where the tab is narrow (#80).
    Pick(u64),
    /// Note `0` opened at its line.
    Open(u64),
    /// For later; its "done" resolved.
    Done,
}

/// How the tab answers clicks on the agents' answers.
pub type OnAnswer = Rc<dyn Fn(AnswerEvent, &mut App)>;

/// Who an answer is from: the chat's agent ("Claude") and the chat's title.
pub struct From {
    pub agent: String,
    pub chat: String,
}

/// What the agent did, as the row says it: asked, your answer waiting to be sent, done.
pub fn verb(c: &Comment) -> &'static str {
    match c.state {
        State::Addressed => "addressed your note",
        State::Draft => "asks you · answered, not sent",
        _ => "asks you",
    }
}

/// Where a note is written: `refunds/`, `Money.kt:23`.
pub fn note_place(c: &Comment) -> String {
    if c.path.ends_with('/') {
        return c.path.clone();
    }
    let name = c.path.rsplit('/').next().unwrap_or(&c.path);
    format!("{name}:{}", c.line)
}

/// How an answer's row looks.
pub struct AnswerLook {
    /// Not looked at since the agent last wrote: bold, its dot purple.
    pub fresh: bool,
    /// Selected.
    pub on: bool,
    /// The list has 260 px (#79): when, and the start of what it wrote.
    pub wide: bool,
}

/// The answer to note `c` (#116): who and what, the note's place, the start of what the agent wrote.
pub fn answer_row(c: &Comment, from: From, look: AnswerLook, on: &OnAnswer) -> Stateful<Div> {
    let AnswerLook {
        fresh,
        on: picked,
        wide,
    } = look;
    let From { agent, chat } = from;
    let id = c.id;
    let at = c.replies.last().map_or(0, |r| r.at);
    let said = c.replies.last().map(|r| plain(&r.body)).unwrap_or_default();
    let gist = said
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned();
    let place = note_place(c);
    let verb = verb(c);
    let mut tip = vec![format!("{agent} {verb}"), place.clone()];
    if !chat.is_empty() {
        tip.push(format!("chat “{chat}”"));
    }
    tip.push(hover::full_date(at));
    if !said.trim().is_empty() {
        tip.push(said.trim().to_owned());
    }
    let on = on.clone();
    div()
        .id(("inbox-agent", id as usize))
        .mx(px(6.))
        .px(px(10.))
        .py(px(8.))
        .flex()
        .items_start()
        .gap(px(10.))
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .when(picked, |s| s.bg(theme::selected()))
        .hover(|s| s.bg(theme::hover()))
        .child(
            div()
                .flex_none()
                .mt(px(6.))
                .size(px(7.))
                .rounded_full()
                .when(fresh, |s| s.bg(theme::agent())),
        )
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
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .gap(px(5.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .when(fresh, |s| s.font_weight(FontWeight::SEMIBOLD))
                                .child(agent)
                                .child(div().text_color(theme::agent()).child(verb)),
                        )
                        .children(wide.then(|| {
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format::ago(at))
                        })),
                )
                .child(
                    div()
                        .cut_in_tip()
                        .text_size(px(12.))
                        .font_family(theme::code_font())
                        .text_color(theme::agent())
                        .child(place),
                )
                .children((wide && !gist.is_empty()).then(|| {
                    div()
                        .cut_in_tip()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(gist)
                })),
        )
        .tip(tip.join("\n"))
        .on_click(move |_: &ClickEvent, _, cx| on(AnswerEvent::Pick(id), cx))
}

/// The answer to note `c` selected: whose and where, the lines the note is on as they were, the purple `thread`
/// itself — your note, what the agent said, its form, your answer and Send, or Resolve —, and the way to its line.
pub fn answer_preview(c: &Comment, from: From, thread: AnyElement, on: &OnAnswer) -> AnyElement {
    let From { agent, chat } = from;
    let id = c.id;
    let at = c.replies.last().map_or(0, |r| r.at);
    let props = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(theme::agent())
                .child(icons::icon("bot").size(px(14.)).text_color(theme::agent()))
                .child(if chat.is_empty() {
                    agent.clone()
                } else {
                    format!("{agent} · {chat}")
                }),
        )
        .children(c.branch.clone().filter(|b| !b.is_empty()).map(|b| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    icons::icon("branch")
                        .size(px(14.))
                        .text_color(theme::muted()),
                )
                .child(div().font_family(theme::code_font()).child(b))
        }))
        .child(div().flex_1())
        .child(format!("{agent} {} · {}", verb(c), format::ago(at)));
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(note_place(c));
    // The lines the note is on, as they were when it was written.
    let code = (!c.path.ends_with('/') && !c.code.is_empty()).then(|| {
        let first = c.line.saturating_sub(c.above.len() as u32);
        let lines = c
            .above
            .iter()
            .chain([&c.code])
            .enumerate()
            .map(|(k, text)| {
                let n = Some(first + k as u32);
                ExcerptLine {
                    kind: LineKind::Context,
                    old: if c.old { n } else { None },
                    new: if c.old { None } else { n },
                    text: text.clone(),
                    on: true,
                }
            })
            .collect();
        excerpt_view(&c.path, c.line, Some(Some(&Excerpt { lines })))
    });
    let (open_on, done_on) = (on.clone(), on.clone());
    let open = bar_button("inbox-agent-open", "Open at the line", "↵", true)
        .on_click(move |_: &ClickEvent, _, cx| open_on(AnswerEvent::Open(id), cx));
    let later = bar_button(
        "inbox-agent-done",
        if c.state == State::Addressed {
            "Resolve"
        } else {
            "Later"
        },
        "E",
        false,
    )
    .on_click(move |_: &ClickEvent, _, cx| done_on(AnswerEvent::Done, cx));
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(28.))
        .py(px(22.))
        .max_w(px(820.))
        .child(props)
        .child(title)
        .children(code)
        .child(div().mx(px(-12.)).child(thread))
        .child(div().flex().gap_2().child(open).child(later))
        .into_any_element()
}
