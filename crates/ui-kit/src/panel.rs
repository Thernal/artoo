//! A page's side panel (#49): one island of sections one under the other, each folding to a header that still
//! says what is in it (a count, the first item, how far the review is). An open section with a list shares the
//! room left with the other (`open_share`); one without takes what its rows need. The features draw their own
//! sections with these; the tab puts them in the panel.

use crate::hover::cut;
use crate::{icons, island, motion, px, theme};
use gpui::{AnyElement, App, ClickEvent, Div, FontWeight, SharedString, Stateful, div, prelude::*};

/// The height of a section's header, folded or open.
pub const HEADER: f32 = 34.;

/// A section's header: its words, folded or open.
pub struct Head {
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

/// Section `id`'s header (`fold-commits`): the fold chevron, its title, what it holds when folded, and on the
/// right its count and controls; a click on it calls `fold`.
pub fn head(id: &'static str, h: Head, fold: impl Fn(&mut App) + 'static) -> Stateful<Div> {
    let folded = h.folded;
    let (title, key) = split_key(&h.title);
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
                        fold(cx)
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
                .child(title.to_uppercase()),
        )
        // The key that gives its list the keyboard, while it has it: a keycap, not words (#208).
        .children(key.map(|key| crate::keys::caps(&key, crate::keys::On::Plain)))
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

/// A title and the key at its end, `Commits · ⌃2` (`Ctrl+2` off a Mac): the key apart.
fn split_key(title: &str) -> (String, Option<String>) {
    match title.rsplit_once(" · ") {
        Some((t, key)) if key.starts_with('⌃') || key.starts_with("Ctrl") => {
            (t.to_owned(), Some(key.to_owned()))
        }
        _ => (title.to_owned(), None),
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn a_titles_key_is_drawn_apart() {
        use super::split_key;
        assert_eq!(
            split_key("Commits · ⌃2"),
            ("Commits".into(), Some("⌃2".into()))
        );
        assert_eq!(
            split_key("Changes ✓ · Ctrl+3"),
            ("Changes ✓".into(), Some("Ctrl+3".into()))
        );
        assert_eq!(split_key("Versions · 3"), ("Versions · 3".into(), None));
    }
}
