//! Two versions' commits paired (#165: the branches' piece in the diff's place while a comparison of versions shows
//! its Commits): unchanged, modified, new, dropped; a modified pair opens its interdiff.

use crate::history::{History, VersionTab};
use artoo_core::git::PairKind;
use artoo_review_api::Review;
use artoo_review_api::diff::{Diff, Header};
use artoo_shell_api::{Piece, Services, Slot};
use artoo_ui_kit::{format, hover, island, island_label, px, row, theme};
use gpui::{App, ClickEvent, Entity, FontWeight, div, prelude::*};
use std::rc::Rc;

/// The pairs of `diff`; `on` opens pair `ix`'s interdiff.
pub fn pairs(diff: &Diff, on: Rc<dyn Fn(usize, &mut App)>) -> impl IntoElement {
    island()
        .flex_1()
        .min_w_0()
        .bg(theme::editor())
        .child(island_label(
            "Commits · = unchanged  ! modified  + new  − dropped",
        ))
        .child(
            div()
                .id("pairs")
                .flex_1()
                .overflow_y_scroll()
                .px(px(6.))
                .pb(px(6.))
                .children(diff.pairs.iter().enumerate().map(|(ix, p)| {
                    let (mark, color, label) = match p.kind {
                        PairKind::Unchanged => ("=", theme::muted(), "unchanged"),
                        PairKind::Modified => ("!", theme::warning(), "modified"),
                        PairKind::New => ("+", theme::added(), "new"),
                        PairKind::Dropped => ("−", theme::removed(), "dropped"),
                    };
                    let ids = match (&p.old, &p.new) {
                        (Some(o), Some(n)) => {
                            format!("{} → {}", format::short(o.id), format::short(n.id))
                        }
                        (Some(o), None) => format::short(o.id),
                        (None, Some(n)) => {
                            format!("{}   → {}", " ".repeat(7), format::short(n.id))
                        }
                        (None, None) => String::new(),
                    };
                    let summary = p
                        .new
                        .as_ref()
                        .or(p.old.as_ref())
                        .map(|c| c.summary.clone())
                        .unwrap_or_default();
                    let moved = match p.moved {
                        0 => String::new(),
                        n if n > 0 => format!("moved ↑{n}"),
                        n => format!("moved ↓{}", -n),
                    };
                    let open = p.kind == PairKind::Modified;
                    let item = row(("pair", ix), false)
                        .h(px(32.))
                        .gap_3()
                        .when(!open, |s| s.cursor_default())
                        .child(
                            div()
                                .w(px(14.))
                                .font_family(theme::code_font())
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color)
                                .child(mark),
                        )
                        .child(
                            div()
                                .w(px(150.))
                                .flex_none()
                                .font_family(theme::code_font())
                                .text_size(px(11.))
                                .text_color(theme::muted())
                                .child(ids),
                        )
                        .child(
                            hover::cut("summary", summary)
                                .flex_1()
                                .when(p.kind == PairKind::Dropped, |s| {
                                    s.line_through().text_color(theme::muted())
                                }),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .child(if moved.is_empty() {
                                    label.to_owned()
                                } else {
                                    format!("{label} · {moved}")
                                }),
                        )
                        .children(open.then(|| {
                            div()
                                .flex_none()
                                .text_color(theme::accent())
                                .child("interdiff →")
                        }))
                        .when(open, |s| {
                            let on = on.clone();
                            s.on_click(move |_: &ClickEvent, _, cx| on(ix, cx))
                        });
                    div().w_full().py(px(1.)).child(item)
                })),
        )
}

/// The pairs in the diff's place while a comparison of versions shows its Commits; a modified one's interdiff the
/// history's (`History::show_interdiff`).
pub fn piece(history: Entity<History>, services: Rc<Services>) -> Piece {
    Piece {
        id: "branches.pairs",
        slot: Slot::Instead,
        draw: Rc::new(move |_, _, cx| {
            if history.read(cx).version_tab != VersionTab::Commits {
                return None;
            }
            let review = services.get::<Rc<dyn Review>>()?;
            let diff = review.shown(cx)?;
            if !matches!(diff.header, Header::Versions { .. }) {
                return None;
            }
            let history = history.downgrade();
            let on = Rc::new(move |pair, cx: &mut App| {
                history
                    .update(cx, |h, cx| {
                        if h.show_interdiff(pair, &*review, cx) {
                            cx.notify();
                        }
                    })
                    .ok();
            });
            Some(pairs(&diff, on).into_any_element())
        }),
    }
}
