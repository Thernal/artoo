//! A monogram: up to two letters of a name on a tint of a colour of its own, in a rounded square (a repository's
//! in the rail).

use crate::hover::Hover as _;
use crate::{icons, px, theme};
use gpui::{ElementId, FontWeight, Rgba, div, prelude::*};

/// Up to two letters for a name: the first of its first two words.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(['-', '_', '.', ' '])
        .filter(|w| !w.is_empty())
        .collect();
    let letters: String = match words.as_slice() {
        [] => "?".to_owned(),
        [one] => one.chars().take(2).collect(),
        [a, b, ..] => a.chars().take(1).chain(b.chars().take(1)).collect(),
    };
    letters.to_uppercase()
}

/// What every avatar of a colour of its own stands on (the owner, 2026-11-27): its colour faint, the letters or the
/// glyph on it in the colour itself — a person's, an agent's, a project's, a source's.
pub fn tint(hue: Rgba) -> Rgba {
    Rgba { a: 0.2, ..hue }
}

/// A colour of its own per name, from the theme's code colours.
pub fn hue(name: &str) -> Rgba {
    let sum = name
        .bytes()
        .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    theme::code([3, 2, 1, 5, 8, 4, 7][sum % 7]).into()
}

/// A colour at `degrees` on the wheel, as bright as the code colours a name is given.
pub fn hue_at(degrees: f32) -> Rgba {
    gpui::hsla(degrees.rem_euclid(360.) / 360., 0.62, 0.62, 1.).into()
}

/// A source's hue: the theme's colour the design gives it, else one of its own by name.
pub fn source_hue(name: &str) -> Rgba {
    match name {
        "GitLab" => theme::code(7).into(),
        "GitHub" => theme::muted(),
        "Jira" => theme::code(2).into(),
        "Sentry" => theme::code(8).into(),
        "Slack" => theme::code(1).into(),
        "Jenkins" => theme::code(5).into(),
        "Confluence" => theme::code(3).into(),
        "Linear" => theme::code(4).into(),
        _ => hue(name),
    }
}

/// Source `name`'s small mark at the end of an Inbox row, before its time: `mark`'s letters, or its glyph, on a tint
/// of `hue`; the tooltip says `From <name>`.
pub fn source_badge(
    id: impl Into<ElementId>,
    mark: &str,
    glyph: Option<&'static str>,
    hue: Rgba,
    name: &str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(14.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .bg(tint(hue))
        .text_color(hue)
        .text_size(px(8.))
        .line_height(px(14.))
        .font_weight(FontWeight::BOLD)
        .map(|badge| match glyph {
            Some(glyph) => badge.child(icons::icon(glyph).size(px(9.)).text_color(hue)),
            None => badge.child(mark.to_owned()),
        })
        .tip(format!("From {name}"))
}

/// The rounded square with `mark`'s letters in `hue` on its tint.
pub fn monogram(mark: &str, hue: Rgba, size: f32) -> gpui::Div {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size * 0.29))
        .bg(tint(hue))
        .text_color(hue)
        .text_size(px(size * 0.35))
        .font_weight(FontWeight::BOLD)
        .child(mark.to_owned())
}

/// A chat's letter in its hue on its tint (the `To:` chip, its list, #200): a rounded square.
pub fn letter(letter: char, hue: Rgba, size: f32) -> gpui::Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 4.))
        .bg(tint(hue))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(size * 0.625))
        .font_weight(FontWeight::BOLD)
        .text_color(hue)
        .child(letter.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_takes_the_first_letters_of_the_first_two_words() {
        assert_eq!(initials("payments-service"), "PS");
        assert_eq!(initials("Desktop"), "DE");
        assert_eq!(initials("artoo_desktop.app"), "AD");
        assert_eq!(initials(""), "?");
        assert_eq!(hue("payments-service"), hue("payments-service"));
    }
}
