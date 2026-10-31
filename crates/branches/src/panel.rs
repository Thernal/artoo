//! The side panel of a branch's or a request's page (#49): one island of sections one under the other — Versions,
//! Commits, Changes — each folding to a header that still says what is in it (a count, the first item, how far the
//! review is). An open Commits or Changes takes the room left; Versions takes what its rows need. The tab says which
//! sections there are, folded or open, and what each holds; a click on a header asks it `Act::Fold`.

use crate::history::{Act, Acts};
use artoo_ui_kit::hover::cut;
use artoo_ui_kit::{icons, island, motion, px, theme};
use gpui::{AnyElement, ClickEvent, Div, FontWeight, SharedString, Stateful, div, prelude::*};

/// The height of a section's header, folded or open.
pub const HEADER: f32 = 34.;

/// A section of the panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    Versions = 0,
    Commits = 1,
    Changes = 2,
}

/// A section's header: which it is, its words, folded or open.
pub struct Head {
    pub fold: Fold,
    pub title: String,
    /// What it holds, said while it is folded.
    pub summary: Option<String>,
    /// At its right: its count and controls.
    pub end: Vec<AnyElement>,
    /// Folded, or it does not open.
    pub folded: bool,
    /// Whether it opens at all (one file's Changes does not).
    pub opens: bool,
}

/// A section's header: the fold chevron, its title, what it holds when folded, and on the right its count and
/// controls.
pub fn head(h: Head, acts: &Acts) -> Stateful<Div> {
    let id = match h.fold {
        Fold::Versions => "fold-versions",
        Fold::Commits => "fold-commits",
        Fold::Changes => "fold-changes",
    };
    let (fold, folded, acts) = (h.fold, h.folded, acts.clone());
    div()
        .id(id)
        .flex_none()
        .h(px(HEADER))
        .pl(px(8.))
        .pr(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .when(h.opens, |s| {
            s.cursor_pointer().on_click(
                // A double click folds or opens it once, as one click does.
                move |e: &ClickEvent, _, cx| {
                    if e.click_count() < 2 {
                        acts(Act::Fold(fold), cx)
                    }
                },
            )
        })
        // The chevron turns as it folds (#56, 1).
        .child(motion::toward(
            icons::icon("chevron").size(px(14.)).text_color(if h.opens {
                theme::faint()
            } else {
                theme::panel()
            }),
            (id, 1usize),
            !folded,
            false,
            motion::FAST,
            |chevron, v| {
                chevron.with_transformation(gpui::Transformation::rotate(gpui::radians(
                    -std::f32::consts::FRAC_PI_2 * (1. - v),
                )))
            },
        ))
        .child(
            div()
                .flex_none()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::muted())
                .child(h.title.to_uppercase()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .text_color(theme::muted())
                .children(
                    h.summary
                        .filter(|_| folded)
                        .map(|said| cut("summary", said)),
                ),
        )
        .children(h.end)
}

/// The panel at `width`, its sections one under the other, outlined while one of its lists has the keyboard;
/// with none of them open (`open_lists`) they sit at the top.
pub fn panel(width: f32, keyboard: bool, sections: Vec<AnyElement>, open_lists: usize) -> Div {
    island()
        .w(px(width))
        .flex_none()
        .h_full()
        .border_2()
        .border_color(if keyboard {
            theme::splitter()
        } else {
            theme::island_border()
        })
        .children(sections)
        // Everything folded: the sections sit at the top.
        .when(open_lists == 0, |s| s.child(div().flex_1()))
}

/// The rule between two sections.
pub fn rule() -> Div {
    div().flex_none().h(px(1.)).bg(theme::island_border())
}

/// A section that shares the panel's height with the other open one: its share grows from nothing (its header
/// alone) as it opens, and back as it folds (#56, 1).
pub fn open_share(section: Div, id: &'static str, open: bool) -> impl IntoElement {
    motion::toward(section, id, open, false, motion::BASE, |s, v| {
        s.flex_grow(v)
            .flex_shrink(1.)
            .flex_basis(px(0.))
            .min_h(px(motion::mix(HEADER, 120., v)))
    })
}

/// A section's count, at the right of its header.
pub fn count(n: String) -> AnyElement {
    div()
        .flex_none()
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(SharedString::from(n))
        .into_any_element()
}
