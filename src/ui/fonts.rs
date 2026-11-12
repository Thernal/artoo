//! Settings → Appearance → Fonts and size (#24 A): the interface's face and the code's, each
//! picked from a menu that draws every face in itself — a few installed ones, and Other… for all
//! of them in ⌘K's window —, the code's size in points, the interface size, its tooltip saying
//! where the narrow layout folds at that size (`narrow.rs`: the steps grow with it).

use super::faces::{CODE_FACES, UI_FACES, fold_line, short_list};
pub(super) use super::faces::{face_family, face_label, installed};
use super::menu::{Act, Entry};
use super::{ROW_RADIUS, Workspace, icons, px, theme};
use artoo_ui_kit::controls::setting_row;
use artoo_ui_kit::controls::{chip, group};
use gpui::{AnyElement, ClickEvent, Context, Pixels, Point, div, prelude::*};

/// The code's size in points at code size `zoom` (%), as the diff draws it.
pub(super) fn code_points(zoom: u16) -> u16 {
    (super::CODE_SIZE * f32::from(zoom) / 100.).round() as u16
}

/// The code size (%) that draws the code at `points`.
pub(super) fn zoom_for(points: u16) -> u16 {
    (f32::from(points) * 100. / super::CODE_SIZE).round() as u16
}

impl Workspace {
    /// Interface and its face, Code with its face and size, Interface size (here, and Set up's
    /// Look).
    pub(super) fn font_rows(&self, cx: &mut Context<Self>) -> [AnyElement; 3] {
        let faces = theme::faces();
        let s = self.settings.clone();
        let ui_zoom =
            group()
                .child(chip("ui-zoom-out", "−", false).on_click(
                    cx.listener(|this, _: &ClickEvent, window, cx| this.ui_zoom_by(-1, window, cx)),
                ))
                .child(
                    chip(
                        "ui-zoom-reset",
                        format!("{} %", s.ui_zoom),
                        s.ui_zoom != 100,
                    )
                    .on_click(cx.listener(
                        |this, _: &ClickEvent, window, cx| this.set_ui_zoom(100, window, cx),
                    )),
                )
                .child(chip("ui-zoom-in", "+", false).on_click(
                    cx.listener(|this, _: &ClickEvent, window, cx| this.ui_zoom_by(1, window, cx)),
                ));
        // When the layout narrows at this size: the size's tooltip, not a paragraph (#202).
        let folds: gpui::SharedString = fold_line(s.ui_zoom).into();
        let ui_zoom = div()
            .id("ui-zoom")
            .child(ui_zoom)
            .tooltip_show_delay(super::hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| super::TipOwned(folds.clone())).into());
        let points = code_points(s.zoom);
        let code_size = group()
            .child(chip("code-size-down", "−", false).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.set_zoom(zoom_for(points.saturating_sub(1)), cx)
                },
            )))
            .child(
                chip("code-size-reset", format!("{points} pt"), s.zoom != 100)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_zoom(100, cx))),
            )
            .child(chip("code-size-up", "+", false).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| this.set_zoom(zoom_for(points + 1), cx),
            )));
        let code = div()
            .flex()
            .items_center()
            .gap_2()
            .child(self.face_picker(true, faces.code.as_deref(), cx))
            .child(code_size);
        // Each off its default carries a dot and Reset (#203): the code's face and size go back together.
        let reset =
            |cx: &mut Context<Self>, act: fn(&mut Self, &mut gpui::Window, &mut Context<Self>)| {
                Some(Box::new(
                    cx.listener(move |this, _: &ClickEvent, window, cx| act(this, window, cx)),
                ) as artoo_ui_kit::controls::Reset)
            };
        let ui_reset = faces
            .ui
            .is_some()
            .then(|| reset(cx, |this, _, cx| this.choose_face(false, None, cx)));
        let code_reset = (faces.code.is_some() || s.zoom != 100).then(|| {
            reset(cx, |this, _, cx| {
                this.choose_face(true, None, cx);
                this.set_zoom(100, cx);
            })
        });
        let size_reset = (s.ui_zoom != 100)
            .then(|| reset(cx, |this, window, cx| this.set_ui_zoom(100, window, cx)));
        [
            setting_row(
                "Interface",
                "Interface",
                "",
                self.face_picker(false, faces.ui.as_deref(), cx),
                ui_reset.flatten(),
            )
            .into_any_element(),
            setting_row(
                "Code",
                "Code",
                "Grows with the interface size.",
                code,
                code_reset.flatten(),
            )
            .into_any_element(),
            setting_row(
                "Interface size",
                "Interface size",
                "⌘+ and ⌘− change it from anywhere; ⌘0 back to 100 %",
                ui_zoom,
                size_reset.flatten(),
            )
            .into_any_element(),
        ]
    }

    /// The face in use, drawn in itself; a click opens the face's menu.
    fn face_picker(
        &self,
        code: bool,
        face: Option<&str>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id(if code { "face-code" } else { "face-ui" })
            .min_w(px(190.))
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(ROW_RADIUS))
            .bg(theme::hover())
            .cursor_pointer()
            .hover(|s| s.bg(theme::selected()))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.))
                    .font_family(face_family(code, face))
                    .child(face_label(code, face)),
            )
            .child(
                icons::icon("chevron")
                    .size(px(12.))
                    .text_color(theme::muted()),
            )
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                this.open_faces(code, ev.position(), cx)
            }))
    }

    /// A face's menu: the default, a few installed faces, each in itself; Other… for all.
    pub(super) fn open_faces(&mut self, code: bool, at: Point<Pixels>, cx: &mut Context<Self>) {
        let faces = theme::faces();
        let chosen = if code { faces.code } else { faces.ui };
        let common: &[&str] = if code { &CODE_FACES } else { &UI_FACES };
        let entries = short_list(common, &installed(cx), chosen.as_deref())
            .into_iter()
            .map(|face| {
                Entry::new(face_label(code, face.as_deref()), {
                    let face = face.clone();
                    Act::new(move |this, _, cx| this.choose_face(code, face.clone(), cx))
                })
                .checked(face == chosen)
                .face(face_family(code, face.as_deref()))
            })
            .collect();
        self.open_titled(
            at,
            vec![
                (Some("Each in its own type"), entries),
                (
                    None,
                    vec![
                        Entry::new(
                            "Other…",
                            Act::new(move |this, _, cx| this.open_all_faces(code, cx)),
                        )
                        .key("any installed face"),
                    ],
                ),
            ],
            cx,
        );
    }

    /// Every face installed, in ⌘K's window.
    pub(super) fn open_all_faces(&mut self, code: bool, cx: &mut Context<Self>) {
        self.search(super::palette_lists::faces_of(code), cx);
    }

    /// Draws the interface or the code in `face` (None: the default) and keeps it.
    pub(super) fn choose_face(&mut self, code: bool, face: Option<String>, cx: &mut Context<Self>) {
        let mut faces = theme::faces();
        match code {
            true => faces.code = face,
            false => faces.ui = face,
        }
        theme::choose_faces(&faces);
        // The code's width is measured again in the new face as the window draws.
        self.relayout_all(cx);
        cx.refresh_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Settings;

    #[test]
    fn the_codes_size_in_points_goes_back_and_forth_through_its_percent() {
        assert_eq!(code_points(100), 12);
        for points in code_points(Settings::ZOOM_MIN)..=code_points(Settings::ZOOM_MAX) {
            let zoom = zoom_for(points).clamp(Settings::ZOOM_MIN, Settings::ZOOM_MAX);
            assert_eq!(code_points(zoom), points, "{points} pt");
        }
    }
}
