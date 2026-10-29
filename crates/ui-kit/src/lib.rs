//! Artoo's design system (#165): the theme and its tokens, the interface size, icons, keys drawn as
//! keycaps, motion, how a row gives way as its list narrows, and the words and formats the window uses.
//! A module draws with this and GPUI alone.

pub mod format;
pub mod hover;
pub mod icons;
pub mod input;
pub mod keys;
pub mod motion;
pub mod room;
pub mod theme;
pub mod words;

use artoo_core::{file_icons, highlight, platform, storage, textedit, themes};

/// The corner of a row, a field, a tooltip.
pub const ROW_RADIUS: f32 = 6.;

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

/// The interface size, in percent; read by `px`.
static UI_ZOOM: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(100);
