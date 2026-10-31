//! Artoo's design system (#165): the theme and its tokens, the interface size, icons, keys drawn as
//! keycaps, motion, how a row gives way as its list narrows, and the words and formats the window uses.
//! A module draws with this and GPUI alone.

pub mod complete;
pub mod controls;
pub mod discussion;
pub mod file_icons;
pub mod format;
pub mod hover;
pub mod icons;
pub mod input;
pub mod keys;
pub mod markdown;
pub mod motion;
pub mod panel;
pub mod people;
pub mod pictures;
pub mod room;
pub mod theme;
pub mod words;

use artoo_core::{highlight, platform, storage, textedit, themes};

/// The corner of a row, a field, a tooltip.
pub const ROW_RADIUS: f32 = 6.;

/// The corner of an island: a panel, a card over the window.
pub const ISLAND_RADIUS: f32 = 10.;

/// A length in logical pixels at the interface size the reader chose (⌘+ ⌘−): every size in the
/// window goes through here, so the whole of it grows and shrinks together.
pub fn px(value: f32) -> gpui::Pixels {
    gpui::px(value * f32::from(UI_ZOOM.load(std::sync::atomic::Ordering::Relaxed)) / 100.)
}

/// The interface size as a factor (1.1 at 110 %): what turns window pixels into `px` units.
pub fn ui_zoom() -> f32 {
    f32::from(UI_ZOOM.load(std::sync::atomic::Ordering::Relaxed)) / 100.
}

/// Makes `percent` the interface size: `px` and the window's rem (what `px_2`, `gap_3` … use).
pub fn apply_ui_zoom(percent: u16, window: &mut gpui::Window) {
    UI_ZOOM.store(percent, std::sync::atomic::Ordering::Relaxed);
    window.set_rem_size(gpui::px(16. * f32::from(percent) / 100.));
}

/// Flat panes (Settings → Appearance): hairlines, no gaps, square corners. Window-wide, like the interface
/// size: every tab and window reads it.
pub fn flat() -> bool {
    FLAT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Makes panes flat or islands, for every tab and window; set when settings load or change.
pub fn set_flat(on: bool) {
    FLAT.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Space between islands; the resize handles live in it. Flat panes have none (`gap`).
pub const GAP: f32 = 8.;

/// The space between panes: `GAP` for islands, none for flat panes.
pub fn gap() -> f32 {
    if flat() { 0. } else { GAP }
}

/// A rounded pane floating on the window background; square and flush when panes are flat.
pub fn island() -> gpui::Div {
    use gpui::{Styled, div};
    div()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded(px(if flat() { 0. } else { ISLAND_RADIUS }))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
}

/// A section's name, small and in capitals.
pub fn section_label(text: impl Into<gpui::SharedString>) -> gpui::Div {
    use gpui::{ParentElement, Styled, div};
    div()
        .flex_none()
        .pb_1()
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(theme::faint())
        .child(text.into().to_uppercase())
}

/// The name at the top of an island, in the words of the app (`words::say`).
pub fn island_label(text: impl Into<gpui::SharedString>) -> gpui::Div {
    use gpui::Styled;
    let text: gpui::SharedString = text.into();
    section_label(gpui::SharedString::from(words::say(&text).into_owned()))
        .px(px(14.))
        .pt(px(10.))
        .pb(px(6.))
}

/// A clickable list row: a rounded pill, filled when selected.
pub fn row(id: impl Into<gpui::ElementId>, selected: bool) -> gpui::Stateful<gpui::Div> {
    use gpui::{InteractiveElement, Styled, div, prelude::FluentBuilder};
    div()
        .id(id)
        .w_full()
        .flex()
        .items_center()
        .px(px(8.))
        .rounded(px(ROW_RADIUS))
        .cursor_pointer()
        .when(selected, |s| s.bg(theme::selected()))
        .when(!selected, |s| s.hover(|s| s.bg(theme::hover())))
}

static FLAT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The interface size, in percent; read by `px`.
static UI_ZOOM: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(100);
