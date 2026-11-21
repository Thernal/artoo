//! The diff's toolbar folds by room, never clips (#187, the rule of #186): what it has no room for goes into View ▾,
//! Collapse all first, then Split · Unified; then the count beside the range goes. The range never folds. Its parts
//! are measured as drawn and the next frame decides; a part folded keeps the width it was last drawn at, so it comes
//! back when that fits.

use artoo_review_api::ToolbarFold;
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::hover::Hover;
use artoo_ui_kit::{icons, keys, px, theme};
use gpui::{AnyElement, ClickEvent, Div, IntoElement, Stateful, canvas, prelude::*};
use std::cell::Cell;

/// A width not measured yet, taken as wide enough: Split · Unified, Collapse all, View ▾.
const GUESS: [f32; 3] = [60., 32., 72.];
/// One control height in a bar (Design `SYSTEM.md`, One control, one source): the group's, the icon buttons' side.
const HEIGHT: f32 = 28.;
/// The toolbar's gap between its parts and its side padding (`render_toolbar`, `render_options`).
const GAP: f32 = 8.;
const PADDING: f32 = 24.;

/// The 28 px track every control of the toolbar's right sits in: Split · Unified, Expand all, View ▾.
pub fn tool_group() -> Div {
    group().h(px(HEIGHT))
}

/// A text chip as tall as the track's inside (24 px), its words centred: View ▾.
pub fn text_chip(id: &'static str, label: &'static str, active: bool) -> Stateful<Div> {
    chip(id, label, active)
        .h(px(HEIGHT - 4.))
        .flex()
        .items_center()
}

/// An icon chip: a square of the track's inside, the glyph centred.
fn glyph(id: &'static str, name: &'static str, active: bool) -> Stateful<Div> {
    let colour = if active {
        theme::text()
    } else {
        theme::muted()
    };
    chip(id, "", active)
        .size(px(HEIGHT - 4.))
        .p_0()
        .flex()
        .items_center()
        .justify_center()
        .child(icons::icon(name).size(px(14.)).text_color(colour))
}

/// Split and Unified, the one drawing of the layout switch (Design round 21, F1; every toolbar that shows it calls
/// this): two glyphs in one 28 px group, a tip each. The one on screen is marked (`unified`); `narrow` dims Split
/// with the room it needs.
pub fn layout(
    unified: bool,
    narrow: bool,
    split: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    one_column: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    tool_group()
        .child(
            glyph("split", "columns", !unified)
                .tip_two("Split", "The old and the new side by side")
                .when(narrow, |c| {
                    c.opacity(0.45).tip_two(
                        "Split",
                        format!("Needs {} px", artoo_review_api::SPLIT_WIDTH as u32),
                    )
                })
                .on_click(split),
        )
        .child(
            glyph("unified", "rows", unified)
                .tip_two("Unified", "One column")
                .on_click(one_column),
        )
        .into_any_element()
}

/// Expand all (`folded`: a file is folded) or Collapse all: one glyph in a 28 px square, its words as the tip.
pub fn expand_all(
    folded: bool,
    click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let (name, tip) = if folded {
        ("chevrons-up-down", "Expand all".to_string())
    } else {
        (
            "chevrons-down-up",
            keys::text("Collapse all  ⌥⇧A").into_owned(),
        )
    };
    tool_group()
        .w(px(HEIGHT))
        .child(glyph("expand-all", name, false).tip(tip).on_click(click))
        .into_any_element()
}

/// What the toolbar was last drawn at, in pixels.
#[derive(Default)]
pub struct ToolRoom {
    /// The toolbar.
    pub width: Cell<f32>,
    /// The range on its left with its count: kept while the count is folded, so it comes back when it fits.
    pub left: Cell<f32>,
    /// Split · Unified, Collapse all, View ▾.
    pub layout: Cell<f32>,
    pub collapse: Cell<f32>,
    pub view: Cell<f32>,
}

/// A width kept, or the guess for it.
fn or(cell: &Cell<f32>, guess: f32) -> f32 {
    match cell.get() {
        w if w > 0. => w,
        _ => guess,
    }
}

impl ToolRoom {
    /// What folds, by the widths last drawn: the first fold at which the toolbar fits, `Count` when none does (the
    /// toolbar then wraps). Nothing folds before it was measured; `collapse` whether Collapse all is drawn at all.
    pub fn fold(&self, collapse: bool) -> ToolbarFold {
        let width = self.width.get();
        if width <= 0. {
            return ToolbarFold::None;
        }
        let room = width - PADDING;
        let left = or(&self.left, 0.);
        let (layout, view) = (or(&self.layout, GUESS[0]), or(&self.view, GUESS[2]));
        let collapse = if collapse {
            or(&self.collapse, GUESS[1]) + GAP
        } else {
            0.
        };
        // The left, the spacer between, then the options: their gaps on both sides of the spacer.
        let need = |left: f32, options: f32| left + GAP * 2. + options;
        [
            (
                ToolbarFold::None,
                need(left, layout + GAP + collapse + view),
            ),
            (ToolbarFold::Collapse, need(left, layout + GAP + view)),
            (ToolbarFold::Layout, need(left, view)),
        ]
        .into_iter()
        .find(|&(_, need)| need <= room)
        .map_or(ToolbarFold::Count, |(fold, _)| fold)
    }
}

/// A canvas over its parent that keeps the parent's width in `keep`, which says whether it changed: the toolbar is
/// drawn again on the next frame then, as what folds was decided by the width before. It draws nothing and takes no
/// pointer.
pub fn measure(keep: impl Fn(f32) -> bool + 'static) -> impl IntoElement {
    canvas(
        move |bounds, window, _| {
            if keep(f32::from(bounds.size.width)) {
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(width: f32) -> ToolRoom {
        let room = ToolRoom::default();
        room.width.set(width);
        room.left.set(260.);
        room.layout.set(110.);
        room.collapse.set(90.);
        room.view.set(70.);
        room
    }

    #[test]
    fn the_toolbar_folds_collapse_then_the_layout_then_the_count() {
        // 24 padding, 260 left, 16 around the spacer: 300 before the options.
        assert_eq!(room(900.).fold(true), ToolbarFold::None);
        assert_eq!(room(560.).fold(true), ToolbarFold::Collapse);
        assert_eq!(room(560.).fold(false), ToolbarFold::None);
        assert_eq!(room(450.).fold(true), ToolbarFold::Layout);
        assert_eq!(room(340.).fold(true), ToolbarFold::Count);
        // Not measured yet: nothing folds.
        assert_eq!(ToolRoom::default().fold(true), ToolbarFold::None);
    }
}
