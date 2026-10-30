//! How images are looked at in a diff — 2-up, swipe or onion skin —, each decoded once, and the words on what
//! changed between the sides.

use artoo_core::git::{FileDiff, ImageKind, ImageSide};
use artoo_ui_kit::{px, theme};
use gpui::{Bounds, ImageFormat, Pixels, div, prelude::*};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// The box an image is drawn in at most, before the interface zoom.
pub const MAX_W: f32 = 720.;
pub const MAX_H: f32 = 520.;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    TwoUp,
    Swipe,
    Onion,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::TwoUp, Mode::Swipe, Mode::Onion];

    pub fn label(self) -> &'static str {
        match self {
            Mode::TwoUp => "2-up",
            Mode::Swipe => "Swipe",
            Mode::Onion => "Onion skin",
        }
    }
}

/// How images are being looked at, and the decoded images kept between frames.
pub struct Images {
    pub mode: Mode,
    /// An SVG shows its text diff.
    pub source: bool,
    /// Where the swipe is, or how opaque the new image is: 0 … 1.
    pub mix: f32,
    /// Where the swipe or onion box was drawn, for the mouse.
    pub area: Rc<Cell<Option<Bounds<Pixels>>>>,
    cache: RefCell<HashMap<usize, Arc<gpui::Image>>>,
}

impl Default for Images {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            source: false,
            mix: 0.5,
            area: Rc::default(),
            cache: RefCell::default(),
        }
    }
}

impl Images {
    /// The image of one side, decoded once per set of bytes.
    pub fn image(&self, side: &ImageSide) -> Arc<gpui::Image> {
        let key = Arc::as_ptr(&side.bytes) as *const u8 as usize;
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 24 {
            cache.clear();
        }
        cache
            .entry(key)
            .or_insert_with(|| {
                Arc::new(gpui::Image::from_bytes(
                    format(side.kind),
                    side.bytes.to_vec(),
                ))
            })
            .clone()
    }
}

pub fn format(kind: ImageKind) -> ImageFormat {
    match kind {
        ImageKind::Png => ImageFormat::Png,
        ImageKind::Jpeg => ImageFormat::Jpeg,
        ImageKind::Gif => ImageFormat::Gif,
        ImageKind::Webp => ImageFormat::Webp,
        ImageKind::Bmp => ImageFormat::Bmp,
        ImageKind::Ico => ImageFormat::Ico,
        ImageKind::Tiff => ImageFormat::Tiff,
        ImageKind::Svg => ImageFormat::Svg,
    }
}

/// `12.3 KB`.
pub fn bytes(n: usize) -> String {
    match n {
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.1} KB", n as f64 / 1024.),
        n => format!("{:.1} MB", n as f64 / (1024. * 1024.)),
    }
}

/// `PNG · 640 × 480 · 12.3 KB`.
pub fn describe(side: &ImageSide) -> String {
    let size = side
        .size
        .map(|(w, h)| format!(" · {w} × {h}"))
        .unwrap_or_default();
    format!("{}{size} · {}", side.kind.label(), bytes(side.bytes.len()))
}

/// What changed between the sides: `640 × 480 → 800 × 600 · +1.8 KB`.
pub fn change(old: &ImageSide, new: &ImageSide) -> String {
    let size = match (old.size, new.size) {
        (Some(a), Some(b)) if a != b => format!("{} × {} → {} × {}", a.0, a.1, b.0, b.1),
        (Some(_), Some(_)) => "same size".to_owned(),
        _ => String::new(),
    };
    let delta = new.bytes.len() as i64 - old.bytes.len() as i64;
    let weight = match delta {
        0 => "same weight".to_owned(),
        d if d > 0 => format!("+{}", bytes(d as usize)),
        d => format!("−{}", bytes((-d) as usize)),
    };
    [size, weight]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The size to draw an image of `size` at: as it is, or scaled down into the box.
pub fn fit(size: Option<(u32, u32)>, max_w: f32, max_h: f32) -> (f32, f32) {
    let (w, h) = size.map_or((max_w, max_h), |(w, h)| (w.max(1) as f32, h.max(1) as f32));
    let scale = (max_w / w).min(max_h / h).min(1.);
    (w * scale, h * scale)
}

/// Whether either side of `file` is an SVG.
pub fn is_svg(file: &FileDiff) -> bool {
    [&file.old_image, &file.new_image]
        .into_iter()
        .flatten()
        .any(|s| s.kind == ImageKind::Svg)
}

/// The border an image sits in, in its side's colour.
pub fn frame(tone: gpui::Rgba) -> gpui::Div {
    div()
        .p(px(8.))
        .rounded(px(6.))
        .border_1()
        .border_color(tone)
        .bg(theme::panel())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(size: (u32, u32), len: usize) -> ImageSide {
        ImageSide {
            kind: ImageKind::Png,
            bytes: vec![0; len].into(),
            size: Some(size),
        }
    }

    #[test]
    fn the_change_says_size_and_weight() {
        assert_eq!(
            change(&side((640, 480), 2048), &side((800, 600), 4096)),
            "640 × 480 → 800 × 600 · +2.0 KB"
        );
        assert_eq!(
            change(&side((16, 16), 300), &side((16, 16), 200)),
            "same size · −100 B"
        );
    }

    #[test]
    fn a_large_image_is_scaled_down_a_small_one_kept() {
        assert_eq!(fit(Some((1440, 520)), 720., 520.), (720., 260.));
        assert_eq!(fit(Some((32, 32)), 720., 520.), (32., 32.));
    }
}
