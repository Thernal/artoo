//! A changed image shown as an image, as GitLab does: the two sides next to each other with their
//! sizes (2-up), the new one wiped over the old (Swipe), or laid over it (Onion skin). An SVG can
//! switch to its source, the text diff.

use super::diff_view::{chip, group};
use super::px;
use super::{Workspace, theme};
use crate::git::{FileDiff, ImageSide};
use artoo_review::image_view::{MAX_H, MAX_W, Mode, change, describe, fit, is_svg};
use gpui::{ClickEvent, Context, MouseButton, ObjectFit, Pixels, div, img, prelude::*};

impl Workspace {
    pub(super) fn render_image(&self, file: &FileDiff, cx: &mut Context<Self>) -> gpui::AnyElement {
        let both = file.old_image.is_some() && file.new_image.is_some();
        let mode = if both {
            self.review.read(cx).images.mode
        } else {
            Mode::TwoUp
        };
        let modes = group().children(Mode::ALL.map(|m| {
            chip(m.label(), m.label(), mode == m)
                .when(!both && m != Mode::TwoUp, |s| s.opacity(0.4))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.review.update(cx, |rv, _| rv.images.mode = m);
                    cx.notify();
                }))
        }));
        let summary = match (&file.old_image, &file.new_image) {
            (Some(old), Some(new)) => change(old, new),
            (None, Some(new)) => format!("Added · {}", describe(new)),
            (Some(old), None) => format!("Deleted · {}", describe(old)),
            (None, None) => String::new(),
        };
        let toolbar = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .px_4()
            .py(px(8.))
            .border_b_1()
            .border_color(theme::island_border())
            .child(modes)
            .children(is_svg(file).then(|| source_switch(false, cx)))
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(summary),
            );
        let body = match (mode, &file.old_image, &file.new_image) {
            (Mode::Swipe | Mode::Onion, Some(old), Some(new)) => {
                self.render_overlay(mode, old, new, cx).into_any_element()
            }
            _ => self.review.read(cx).render_two_up(file).into_any_element(),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(toolbar)
            .child(
                div()
                    .id("image-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_4()
                    .flex()
                    .justify_center()
                    .child(body),
            )
            .into_any_element()
    }

    /// The new side over the old: wiped in from the left up to the handle (Swipe), or faded in
    /// (Onion skin). A drag in the box or on the bar moves it.
    fn render_overlay(
        &self,
        mode: Mode,
        old: &ImageSide,
        new: &ImageSide,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // One box for both, sized by the larger side.
        let size = match (old.size, new.size) {
            (Some(a), Some(b)) => Some((a.0.max(b.0), a.1.max(b.1))),
            (a, b) => a.or(b),
        };
        let (w, h) = fit(size, MAX_W, MAX_H);
        let mix = self.review.read(cx).images.mix;
        let picture = |side: &ImageSide| {
            img(self.review.read(cx).images.image(side))
                .absolute()
                .top_0()
                .left_0()
                .w(px(w))
                .h(px(h))
                .object_fit(ObjectFit::Contain)
        };
        let area = self.review.read(cx).images.area.clone();
        let (down, drag) = (cx.entity().downgrade(), cx.entity().downgrade());
        let set = |this: &mut Workspace, x: Pixels, cx: &mut Context<Workspace>| {
            if let Some(b) = this.review.read(cx).images.area.get() {
                let at = f32::from(x - b.origin.x) / f32::from(b.size.width).max(1.);
                this.review
                    .update(cx, |rv, _| rv.images.mix = at.clamp(0., 1.));
                cx.notify();
            }
        };
        let stage = div()
            .relative()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(px(4.))
            .overflow_hidden()
            .bg(theme::panel())
            .border_1()
            .border_color(theme::island_border())
            .child(picture(old))
            .child(match mode {
                Mode::Swipe => div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h(px(h))
                    .w(px(w * mix))
                    .overflow_hidden()
                    .child(picture(new))
                    .into_any_element(),
                _ => picture(new).opacity(mix).into_any_element(),
            })
            .when(mode == Mode::Swipe, |s| {
                s.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(w * mix - 1.))
                        .w(px(2.))
                        .h(px(h))
                        .bg(theme::accent()),
                )
            })
            .child(
                gpui::canvas(move |bounds, _, _| area.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
            .cursor_col_resize()
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                down.update(cx, |this, cx| set(this, event.position.x, cx))
                    .ok();
            })
            .on_mouse_move(move |event, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    drag.update(cx, |this, cx| set(this, event.position.x, cx))
                        .ok();
                }
            });
        let (left, right) = match mode {
            Mode::Swipe => ("After ←", "→ Before"),
            _ => ("Before", "After"),
        };
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(stage)
            .child(
                div()
                    .w(px(w.max(240.)))
                    .flex()
                    .justify_between()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(left)
                    .child(format!("{}%", (mix * 100.).round()))
                    .child(right),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child("Drag across the image to move it"),
            )
    }
}

/// Image | Source, for an SVG.
pub(super) fn source_switch(source: bool, cx: &mut Context<Workspace>) -> impl IntoElement {
    group()
        .child(chip("svg-image", "Image", !source).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.review.update(cx, |rv, _| rv.images.source = false);
                cx.notify();
            },
        )))
        .child(chip("svg-source", "Source", source).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.review.update(cx, |rv, _| rv.images.source = true);
                cx.notify();
            },
        )))
}
