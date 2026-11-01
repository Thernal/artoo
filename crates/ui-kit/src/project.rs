//! A project's mark: the rounded square with its initials on a tint of its colour, as the rail, the spaces' menu
//! and the welcome draw it.

use crate::{px, theme};
use gpui::{FontWeight, Rgba, div, prelude::*};

/// Up to two letters for a repository: the first of its first two words.
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

/// A colour of its own per repository name, from the theme's code colours.
pub fn hue(name: &str) -> Rgba {
    let sum = name
        .bytes()
        .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    theme::code([3, 2, 1, 5, 8, 4, 7][sum % 7]).into()
}

/// Project `root`'s colour: the hue it was given (the developer, 2026-10-23), else one of its own
/// by its folder's name, so naming it does not change it.
pub fn project_hue(root: Option<&std::path::Path>, name: &str) -> Rgba {
    let folder = root
        .and_then(|r| r.file_name())
        .map(|n| n.to_string_lossy().into_owned());
    match root.and_then(|r| artoo_core::labels::project(r).hue) {
        Some(degrees) => hue_at(degrees),
        None => hue(folder.as_deref().unwrap_or(name)),
    }
}

/// A colour at `degrees` on the wheel, as bright as the code colours a project is given.
pub fn hue_at(degrees: f32) -> Rgba {
    gpui::hsla(degrees.rem_euclid(360.) / 360., 0.62, 0.62, 1.).into()
}

/// The rounded square with a project's initials on a tint of its colour.
pub fn project_mark(mark: &str, hue: Rgba, size: f32) -> gpui::Div {
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size * 0.29))
        .bg(Rgba { a: 0.35, ..hue })
        .text_color(theme::text())
        .text_size(px(size * 0.35))
        .font_weight(FontWeight::BOLD)
        .child(mark.to_owned())
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
