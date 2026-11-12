//! A monogram: up to two letters of a name on a tint of a colour of its own, in a rounded square (a repository's
//! in the rail).

use crate::{px, theme};
use gpui::{FontWeight, Rgba, div, prelude::*};

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

/// The rounded square with `mark`'s letters on a tint of `hue`.
pub fn monogram(mark: &str, hue: Rgba, size: f32) -> gpui::Div {
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

/// A chat's letter on its hue (the `To:` chip, its list, #200): a solid rounded square, the letter in the base colour.
pub fn letter(letter: char, hue: Rgba, size: f32) -> gpui::Div {
    div()
        .flex_none()
        .size(px(size))
        .rounded(px(size / 4.))
        .bg(hue)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(size * 0.625))
        .font_weight(FontWeight::BOLD)
        .text_color(theme::base())
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
