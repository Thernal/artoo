//! The code a discussion is on, as a card (the Inbox, #49): its path and line, then the lines around it as the
//! change made them, the commented one marked.

use crate::{hover, icons, motion, px, theme};
use artoo_core::git::LineKind;
use gpui::{Div, div, prelude::*};

/// The code a discussion is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Excerpt {
    pub lines: Vec<ExcerptLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExcerptLine {
    pub kind: LineKind,
    pub old: Option<u32>,
    pub new: Option<u32>,
    pub text: String,
    /// The line the discussion is on.
    pub on: bool,
}

/// The card of the code a discussion is on: the path and line, then the lines (or a word that
/// they are not here).
/// `ex`: None while it is being read, `Some(None)` when the commit is not here.
pub fn excerpt_view(path: &str, line: u32, ex: Option<Option<&Excerpt>>) -> Div {
    let head = div()
        .flex()
        .items_center()
        .gap_2()
        .px(px(12.))
        .py(px(7.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            icons::icon("files")
                .size(px(14.))
                .text_color(theme::muted()),
        )
        .child(hover::cut("excerpt-path", path.to_owned()).flex_1())
        .child(format!("line {line}"));
    let body = match ex {
        None => div()
            .px(px(12.))
            .pt(px(4.))
            .pb(px(12.))
            .child(motion::shimmer("inbox-excerpt-loading", 4)),
        Some(Some(ex)) => div()
            .pb(px(6.))
            .font_family(theme::code_font())
            .text_size(px(12.))
            .children(ex.lines.iter().map(|l| {
                let number = |n: Option<u32>| {
                    div()
                        .w(px(36.))
                        .flex_none()
                        .text_right()
                        .pr(px(8.))
                        .text_color(theme::faint())
                        .child(n.map(|n| n.to_string()).unwrap_or_default())
                };
                let sign = match l.kind {
                    LineKind::Added => "+",
                    LineKind::Removed => "−",
                    LineKind::Context => " ",
                };
                div()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .map(|s| match l.kind {
                        LineKind::Added => s.bg(theme::added_line()),
                        LineKind::Removed => s.bg(theme::removed_line()),
                        LineKind::Context => s,
                    })
                    .border_l(px(3.))
                    .border_color(if l.on {
                        theme::warning()
                    } else {
                        gpui::transparent_black().into()
                    })
                    .child(number(l.old))
                    .child(number(l.new))
                    .child(div().w(px(16.)).flex_none().child(sign))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(theme::code_text())
                            .child(l.text.clone()),
                    )
            })),
        Some(None) => div()
            .px(px(12.))
            .pb(px(10.))
            .text_size(px(12.))
            .text_color(theme::faint())
            .child("The commit is not in this repository yet — fetch to see the code."),
    };
    div()
        .max_w(px(720.))
        .flex()
        .flex_col()
        .rounded(px(10.))
        .overflow_hidden()
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .child(head)
        .child(body)
}
