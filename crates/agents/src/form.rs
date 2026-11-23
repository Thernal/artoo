//! An agent's form under its question (#10 C): the choices it offers — options, a file's lines, an order — answered
//! with clicks, one's own value, a note, the foot that sends; under an earlier question, its form as it was answered.
//! What a click asks is the tab's (`FormEvent`): the answer goes to the drafts, the composer writes the rest.

use crate::review::{Answered, Comment, Form, State};
use artoo_ui_kit::{ROW_RADIUS, hover, keys, motion, px, theme};
use gpui::{AnyElement, FontWeight, div, prelude::*};

/// What a click on a form asks, the contract's.
pub use artoo_agents_api::view::{FormEvent, OnForm};

/// Under an earlier question of the agent's: its form as it was, what was picked marked, and
/// the answer in words — or that none was given before it asked again. Only the last question
/// is answered here (the developer, 2026-10-23: an earlier one's options went).
pub fn record(form: &Form, answered: Option<&Answered>) -> AnyElement {
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
                .child(hover::cut(
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
                .child(said.unwrap_or_else(|| "Not answered — the agent asked again".to_owned())),
        )
        .into_any_element()
}

/// Under the agent's question: its form, answered with clicks; the foot sends (#10 C).
/// `lines` the file's own lines, when it is the one open.
pub fn form(
    c: &Comment,
    _drafts: &[u64],
    lines: Option<&dyn Fn(u32) -> Option<String>>,
    on: OnForm,
) -> Option<AnyElement> {
    form_at(c, lines, on, false)
}

/// `form` as the conversation draws it (#220): inside the question's own card, so no margin of its own and no
/// border.
pub fn bare(
    c: &Comment,
    lines: Option<&dyn Fn(u32) -> Option<String>>,
    on: OnForm,
) -> Option<AnyElement> {
    form_at(c, lines, on, true)
}

fn form_at(
    c: &Comment,
    lines: Option<&dyn Fn(u32) -> Option<String>>,
    on: OnForm,
    bare: bool,
) -> Option<AnyElement> {
    use crate::review::FormKind;
    let id = c.id;
    if !matches!(c.state, State::Sent | State::Draft) {
        return None;
    }
    let reply = c.replies.last()?;
    let form = reply.form.as_ref();
    let answer = c.answer.clone().unwrap_or_default();
    let answered = c.answer.is_some();
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
        Some(f) if f.kind == FormKind::Line => line_choices(c, f, &answer, lines, on.clone()),
        _ => option_rows(id, form, &order, &answer, answered, &mark, on.clone()),
    };
    // Text: one's own value beside the suggestions (#10 C).
    let own = form.filter(|f| f.kind == FormKind::Text).map(|_| {
        let edit = on.clone();
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
                edit(FormEvent::Own(id), cx);
            })
            .into_any_element()
    });
    let (call, note, only) = (on.clone(), on.clone(), on);
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
            .when(!bare, |s| {
                s.mx(px(12.))
                    .mb(px(10.))
                    .ml(px(52.))
                    .p(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(gpui::Rgba {
                        a: 0.35,
                        ..theme::agent()
                    })
            })
            .flex()
            .flex_col()
            .gap(px(4.))
            .children(rows)
            .children(own)
            // The last choice, and a line that is always there (L1): `0 Up to you · Claude decides`.
            .child(
                div()
                    .id(("form-call", id as usize))
                    .mx(px(8.))
                    .h(px(28.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(ROW_RADIUS))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .when(answer.your_call, |s| s.bg(theme::agent_bg()))
                    .child(
                        div()
                            .w(px(14.))
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child("0"),
                    )
                    .child(mark(answer.your_call, false))
                    .child("Up to you")
                    .child(div().text_color(theme::faint()).child("Claude decides"))
                    .on_click(move |_, _, cx| {
                        call(FormEvent::Pick(id, None), cx);
                    }),
            )
            .child(
                div()
                    .id(("form-note", id as usize))
                    .mx(px(8.))
                    .mt(px(4.))
                    .min_h(px(30.))
                    .px(px(10.))
                    .py(px(5.))
                    .flex()
                    .items_center()
                    .rounded(px(ROW_RADIUS))
                    .border_1()
                    .border_color(theme::island_border())
                    .bg(theme::editor())
                    .cursor_text()
                    .text_size(px(12.))
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| {
                        cx.new(|_| hover::Tip("Add a note to your answer  ⇧N"))
                            .into()
                    })
                    .child(if answer.note.is_empty() {
                        div()
                            .text_color(theme::faint())
                            .child("Anything to add — optional")
                    } else {
                        div().child(answer.note.clone())
                    })
                    .on_click(move |_, _, cx| {
                        note(FormEvent::Note(id), cx);
                    }),
            )
            // An answer goes at once (round 11): the foot says so, and `Send answer ↵` is the whole of it.
            .child(
                div()
                    .pt(px(6.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child("Goes to your agent at once"),
                    )
                    .child(
                        button(
                            ("form-all", id as usize),
                            keys::text("Send answer  ↵").into_owned(),
                            true,
                        )
                        .h(px(28.))
                        .when(!answered, |s| s.opacity(0.5))
                        .when(answered, |s| {
                            s.on_click(move |_, _, cx| {
                                only(FormEvent::SendOnly(id), cx);
                            })
                        }),
                    ),
            )
            .into_any_element(),
    )
}

/// Line: the lines the agent suggests drawn as code, with the lines between them from the
/// file (when it is the one open); a suggested line is picked with a click (Design
/// `kinds.html`, line).
fn line_choices(
    c: &Comment,
    f: &Form,
    answer: &Answered,
    lines: Option<&dyn Fn(u32) -> Option<String>>,
    on: OnForm,
) -> Vec<AnyElement> {
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
    let text_of = |n: u32| -> Option<String> {
        suggested
            .iter()
            .find(|(_, m, code)| *m == n && !code.is_empty())
            .map(|(_, _, code)| code.clone())
            .or_else(|| lines?(n))
    };
    let span = if lines.is_some() && high - low <= 12 {
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
        let pick = on.clone();
        Some(
            div()
                .id(("form-line", n as usize))
                .min_h(px(20.))
                .flex()
                .items_start()
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
                // An option wraps to two lines, then ends in an ellipsis with the whole on hover (#184).
                .child(hover::cut_to(("form-line-text", n as usize), text, 2).flex_1())
                .children(picked.then(|| {
                    div()
                        .flex_none()
                        .font_family(theme::ui_font())
                        .text_size(px(11.))
                        .text_color(theme::agent())
                        .child("← here")
                }))
                .when_some(choice, |s, i| {
                    s.cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .on_click(move |_, _, cx| {
                            pick(FormEvent::Pick(id, Some(i)), cx);
                        })
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

/// The options of a form, one row each: number, mark, label, detail, code (Order: ↑ ↓).
fn option_rows(
    id: u64,
    form: Option<&Form>,
    order: &[usize],
    answer: &Answered,
    answered: bool,
    mark: &dyn Fn(bool, bool) -> gpui::Div,
    act: OnForm,
) -> Vec<AnyElement> {
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
            let (pick, up, down) = (act.clone(), act.clone(), act.clone());
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
                        .child(div().flex_1().min_w_0().child(o.label.clone()))
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
                                            up(FormEvent::Move(id, place, true), cx);
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
                                            down(FormEvent::Move(id, place, false), cx);
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
                        pick(FormEvent::Pick(id, Some(i)), cx);
                    })
                });
            // The option picked lights up (#56, 17).
            let tint = theme::agent_bg();
            Some(
                motion::toward(
                    row,
                    ("form-option-on", id as usize * 64 + i),
                    on,
                    false,
                    motion::FAST,
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
