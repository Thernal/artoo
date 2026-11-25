//! Your notes an agent answered, in the Inbox (#116): purple where a notice is blue. A row says what it is about (the
//! request, else the note's place) and the start of what the agent wrote; the answer selected shows the request, its
//! thread (the tab's) and the way to its line. A note on no request shows its place and its lines instead. A click asks the tab an `AnswerEvent`.

use crate::review::{Comment, State};
use artoo_core::git::LineKind;
use artoo_core::markdown::plain;
use artoo_ui_kit::controls::bar_button;
use artoo_ui_kit::excerpt::{Excerpt, ExcerptLine, excerpt_view};
use artoo_ui_kit::hover::{self, CutInTip, Hover as _, RowTip};
use artoo_ui_kit::{ROW_RADIUS, discussion, format, icons, px, theme, words};
use gpui::{AnyElement, ClickEvent, Div, FontWeight, Stateful, div, prelude::*};

/// What an answer is drawn with and a click on it asks, the contract's.
pub use artoo_agents_api::view::{About, AnswerEvent, AnswerLook, From, OnAnswer};

/// What the agent did, as the row's tip says it: asked, your answer waiting to be sent, done.
pub use artoo_agents_api::comment::verb;

/// Where a note is written: `refunds/`, `Money.kt:23`, a plugin's place (`PAY-412 · description · ¶4`, #212).
pub fn note_place(c: &Comment) -> String {
    c.reads()
}

/// What an answer is about, as its row's title says it: `!398 Refactor the login screen state`, else the note's
/// place.
fn about_title(c: &Comment, about: Option<&About>) -> String {
    match about {
        Some(a) => format!("{} {}", words::number(a.number), a.title),
        None => note_place(c),
    }
}

/// The answer to note `c` (#116): what it is about, the start of what the agent wrote; who, what and where in the tip.
pub fn answer_row(c: &Comment, from: From, look: AnswerLook, on: &OnAnswer) -> Stateful<Div> {
    let AnswerLook {
        fresh,
        on: picked,
        wide,
    } = look;
    let From { agent, chat, about } = from;
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
    let title = about_title(c, about.as_ref());
    let mut tip = vec![format!("{agent} {verb}"), place];
    if !chat.is_empty() {
        tip.push(format!("chat “{chat}”"));
    }
    tip.push(hover::full_date(at));
    if !said.trim().is_empty() {
        tip.push(said.trim().to_owned());
    }
    let on = on.clone();
    // The Inbox's own row (`artoo_requests::inbox_view`): a face, the title and the short time, the gist.
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
        // Claude's mark: the bot in the agent's colour.
        .child(
            div()
                .flex_none()
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(gpui::Rgba {
                    a: 0.22,
                    ..theme::agent()
                })
                .child(icons::icon("bot").size(px(12.)).text_color(theme::agent())),
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
                                .cut_in_tip()
                                .text_color(if fresh { theme::text() } else { theme::muted() })
                                .when(fresh, |s| s.font_weight(FontWeight::SEMIBOLD))
                                .child(title),
                        )
                        .child(
                            format::time_cell(at)
                                .text_size(px(11.))
                                .text_color(theme::faint()),
                        ),
                )
                .children((wide && !gist.is_empty()).then(|| {
                    div()
                        .cut_in_tip()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(gist)
                })),
        )
        .row_tip(picked, tip.join("\n"))
        .on_click(move |_: &ClickEvent, _, cx| on(AnswerEvent::Pick(id), cx))
}

/// The answer to note `c` selected: the request's number, author and title, the purple `thread` itself — your note,
/// what the agent said, its form, your answer and Send, or Resolve —, and the way to its line. On no request the head
/// says whose and where, and the lines the note is on are shown as they were.
pub fn answer_preview(c: &Comment, from: From, thread: AnyElement, on: &OnAnswer) -> AnyElement {
    let From { agent, chat, about } = from;
    let id = c.id;
    let at = c.replies.last().map_or(0, |r| r.at);
    let when = div()
        .id("inbox-agent-when")
        .child(format::short_ago(at))
        .tip(hover::full_date(at));
    let props = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_2()
        .text_size(px(12.))
        .text_color(theme::muted());
    // As a request's notice heads its preview (#202): the number, its author, when.
    let props = match &about {
        Some(a) => props
            .child(
                div()
                    .font_family(theme::code_font())
                    .text_color(theme::accent())
                    .child(words::number(a.number)),
            )
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.))
                    .child(discussion::avatar(&a.author, false, 16.))
                    .child(div().flex_none().child(a.author.clone())),
            )
            .child(when),
        None => props
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
                    })
                    .child(div().text_color(theme::agent()).child(verb(c))),
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
            .child(when.text_color(theme::faint())),
    };
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(
            about
                .as_ref()
                .map_or_else(|| note_place(c), |a| a.title.clone()),
        );
    // The lines the note is on, as they were when it was written; on a request the thread's head names the place.
    let lines = about.is_none() && !c.is_folder() && c.on.is_none() && !c.code.is_empty();
    let code = lines.then(|| {
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
    // The question is asked on its line: the button says where it leads (I1).
    let asking = c.state != State::Addressed && c.replies.last().is_some_and(|r| r.form.is_some());
    let open = bar_button(
        "inbox-agent-open",
        if asking {
            "Open at the question"
        } else {
            "Open at the line"
        },
        "↵",
        true,
    )
    .on_click(move |_: &ClickEvent, _, cx| open_on(AnswerEvent::Open(id), cx));
    let later = bar_button("inbox-agent-done", "Done", "E", false)
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
