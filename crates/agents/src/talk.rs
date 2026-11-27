//! The agent's words and its plans as a session's discussions draw them (`crate::threads`): each `path:line` in its
//! words drawn in its sentence (hovered, the lines around; a click opens it in Code), a plan's steps with Approve.

use crate::review::Comment;
use artoo_agents_api::view::{Act, AgentsHost, FormEvent};
use artoo_ui_kit::ROW_RADIUS;
use artoo_ui_kit::discussion::{self, Kind as Button};
use artoo_ui_kit::{format, hover, icons, markdown, px, theme};
use gpui::{AnyElement, App, ClickEvent, FontWeight, SharedString, div, prelude::*};
use std::rc::Rc;

/// The open request a comment was written on: the one its branch is the source of. A plugin's place names none.
pub(crate) fn request_of(c: &Comment, host: Option<&Rc<dyn AgentsHost>>, cx: &App) -> Option<u64> {
    let branch = c.branch.as_deref().filter(|_| c.on.is_none())?;
    host?.request(branch, cx)
}

/// A comment's place, short: `LoginViewModel.kt:51`, on a request `!398 · LoginViewModel.kt:51`.
pub(crate) fn place(c: &Comment, request: Option<u64>) -> String {
    match request {
        Some(n) => format!("{} · {}", artoo_ui_kit::words::number(n), c.reads()),
        None => c.reads(),
    }
}

/// A plan waiting for you: `Plan  3 steps`, its steps numbered (a click leaves one out or takes it in), `Approve`
/// and the pencil that changes it; `host` answers the clicks.
pub(crate) fn plan_of(c: &Comment, host: Option<Rc<dyn AgentsHost>>) -> AnyElement {
    let id = c.id;
    let Some(form) = c.replies.last().and_then(|r| r.form.as_ref()) else {
        return div().into_any_element();
    };
    let on_form = |event: FormEvent| {
        let host = host.clone();
        move |_: &ClickEvent, _: &mut gpui::Window, cx: &mut App| {
            if let Some(host) = &host {
                host.act(Act::Form(event), cx);
            }
        }
    };
    let steps = form.choices().into_iter().enumerate().map(|(i, o)| {
        let on = c.answer.as_ref().map_or(o.on, |a| a.picks.contains(&i));
        div()
            .id(("talk-step", id as usize * 64 + i))
            .flex()
            .gap(px(8.))
            .px(px(4.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .hover(|d| d.bg(theme::hover()))
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    hover::Tip(if on {
                        "Leave this step out"
                    } else {
                        "Take this step in"
                    })
                })
                .into()
            })
            .child(
                div()
                    .w(px(16.))
                    .flex_none()
                    .text_color(theme::faint())
                    .child(format!("{}.", i + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .when(!on, |d| d.line_through().text_color(theme::faint()))
                    .child(o.label.clone()),
            )
            .on_click(on_form(FormEvent::Pick(id, Some(i))))
    });
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .child(icons::icon("list").size(px(14.)).text_color(theme::muted()))
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Plan"))
                .child(
                    div()
                        .text_color(theme::muted())
                        .child(format::plural(form.options.len(), "step")),
                ),
        )
        .children(steps)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .pt(px(2.))
                .child(
                    discussion::button(
                        ("talk-approve", id),
                        "Approve",
                        None,
                        Button::Primary(theme::accent()),
                    )
                    .h(px(28.))
                    .px(px(10.))
                    .tooltip_show_delay(hover::LONG)
                    .tooltip(|_, cx| {
                        cx.new(|_| hover::Tip("Approve the plan: Claude starts on it"))
                            .into()
                    })
                    .on_click(on_form(FormEvent::Approve(id))),
                )
                .child(
                    discussion::icon_button(
                        ("talk-change", id),
                        "pencil",
                        "Change the plan: say what, in the composer",
                        None,
                        None,
                        Button::Plain,
                        true,
                    )
                    .h(px(28.))
                    .px(px(8.)),
                ),
        )
        .into_any_element()
}

/// The agent's words, each `path:line` of the folder they name drawn in its sentence: hovered, the lines around;
/// a click opens it in Code.
pub(crate) fn words(
    key: usize,
    text: &str,
    folder: &std::path::Path,
    host: Option<Rc<dyn AgentsHost>>,
    cx: &App,
) -> AnyElement {
    let text = crate::cites::inline(text, folder);
    let (open, card) = (host.clone(), host);
    let folder = folder.to_path_buf();
    let cites = markdown::Cites {
        open: Rc::new(move |address, cx| {
            if let (Some(host), Some(cite)) = (&open, crate::cites::of_address(address)) {
                host.act(
                    Act::OpenCode {
                        path: cite.path,
                        line: cite.line,
                    },
                    cx,
                );
            }
        }),
        card: Rc::new(move |address, cx| {
            let cite = crate::cites::of_address(address).unwrap_or(crate::cites::Cite {
                path: String::new(),
                line: 0,
            });
            let changed = card
                .as_ref()
                .and_then(|h| h.noted(&cite.path, cite.line, cx));
            let lines = crate::cites::around(&folder, &cite);
            cx.new(|_| CiteCard {
                foot: foot(cite.line, changed),
                path: cite.path,
                lines,
            })
            .into()
        }),
    };
    markdown::body_cited(&text, format!("say-{key}"), cites, cx)
}

/// The card's last line: when the agent's own note on its changes named the line, that; always what a click does.
fn foot(line: u32, changed: Option<i64>) -> String {
    match changed {
        Some(at) => format!(
            "Claude changed line {line} at {} · a click opens it in Code",
            format::clock(at)
        ),
        None => "A click opens it in Code".to_owned(),
    }
}

/// The hover card of a `path:line`: the path, the lines around it with the named one marked.
struct CiteCard {
    path: String,
    lines: Vec<(u32, String, bool)>,
    foot: String,
}

impl Render for CiteCard {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        hover::card_frame()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(theme::muted())
                    .child(icons::icon("file").size(px(13.)).text_color(theme::faint()))
                    .child(SharedString::from(self.path.clone())),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(self.lines.iter().map(|(n, text, own)| {
                        div()
                            .flex()
                            .gap(px(8.))
                            .font_family(theme::code_font())
                            .text_size(px(11.))
                            .when(*own, |d| d.bg(theme::agent_bg()))
                            .child(
                                div()
                                    .w(px(28.))
                                    .flex_none()
                                    .text_color(theme::faint())
                                    .child(SharedString::from(n.to_string())),
                            )
                            .child(SharedString::from(text.clone()))
                    })),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(SharedString::from(self.foot.clone())),
            )
    }
}
