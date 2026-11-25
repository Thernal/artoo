//! Settings → Appearance → Fonts and size (#24 A): the interface's face and the code's, each
//! picked from a menu that draws every face in itself — a few installed ones, and Other… for all
//! of them in ⌘K's window —, the interface size, its row's tooltip saying where the narrow layout folds
//! at that size (`narrow.rs`: the steps grow with it). Set up's Look has a size in points beside each face
//! and the interface size as a menu.

use super::faces::{CODE_FACES, UI_FACES, fold_line, short_list};
pub(super) use super::faces::{face_family, face_label, installed};
use super::menu::{Act, Entry};
use super::{ROW_RADIUS, Workspace, icons, px, theme};
use artoo_ui_kit::controls::setting_row;
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::hover::Hover as _;
use gpui::{AnyElement, ClickEvent, Context, Pixels, Point, SharedString, div, prelude::*};

/// The interface's text in points at 100 %.
const UI_POINTS: f32 = 13.;
/// The interface sizes (%) a menu offers.
const UI_ZOOMS: [u16; 8] = [80, 90, 100, 110, 120, 130, 150, 200];
/// The code sizes in points a menu offers.
const CODE_POINTS: [u16; 9] = [10, 11, 12, 13, 14, 15, 16, 18, 20];

/// The interface's text in points at interface size `zoom` (%).
fn ui_points(zoom: u16) -> u16 {
    (UI_POINTS * f32::from(zoom) / 100.).round() as u16
}

/// A menu's values: those offered and the one in use, in order.
fn offered(values: &[u16], now: u16) -> Vec<u16> {
    let mut all = values.to_vec();
    if let Err(at) = all.binary_search(&now) {
        all.insert(at, now);
    }
    all
}

/// The code's size in points at code size `zoom` (%), as the diff draws it.
pub(super) fn code_points(zoom: u16) -> u16 {
    (super::CODE_SIZE * f32::from(zoom) / 100.).round() as u16
}

/// The code size (%) that draws the code at `points`.
pub(super) fn zoom_for(points: u16) -> u16 {
    (f32::from(points) * 100. / super::CODE_SIZE).round() as u16
}

impl Workspace {
    /// Interface and its face, Code and its face, Interface size; `sized`, as Set up's Look draws them: each face
    /// with its size in points, the interface size a menu.
    pub(super) fn font_rows(&self, sized: bool, cx: &mut Context<Self>) -> [AnyElement; 3] {
        let faces = theme::faces();
        let s = self.settings.clone();
        let ui_zoom =
            group()
                .child(
                    chip("ui-zoom-out", "−", false)
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.ui_zoom_by(-1, window, cx)
                        }))
                        .tip("Smaller ⌘−, from anywhere"),
                )
                .child(
                    chip(
                        "ui-zoom-reset",
                        format!("{} %", s.ui_zoom),
                        s.ui_zoom != 100,
                    )
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.set_ui_zoom(100, window, cx)
                    }))
                    .tip("⌘0 back to 100 %, from anywhere"),
                )
                .child(
                    chip("ui-zoom-in", "+", false)
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.ui_zoom_by(1, window, cx)
                        }))
                        .tip_two("Larger ⌘+", "From anywhere; ⌘0 back to 100 %"),
                );
        // Off 100 %, when the layout narrows at this size: the row's tooltip, not a paragraph (#202).
        let size_line: gpui::SharedString = match s.ui_zoom {
            100 => "⌘+ and ⌘− change it from anywhere; ⌘0 back to 100 %".into(),
            zoom => fold_line(zoom).into(),
        };
        let size_line: SharedString = match (sized, s.ui_zoom) {
            (true, 100) => "⌘+ and ⌘− from anywhere".into(),
            _ => size_line,
        };
        let face = |code: bool, face: Option<&str>, cx: &mut Context<Self>| {
            let size = sized.then(|| {
                let (id, points) = match code {
                    true => ("size-code", code_points(s.zoom)),
                    false => ("size-ui", ui_points(s.ui_zoom)),
                };
                size_picker(id, 54., points.to_string()).on_click(cx.listener(
                    move |this, ev: &ClickEvent, _, cx| match code {
                        true => this.open_code_sizes(ev.position(), cx),
                        false => this.open_ui_sizes(true, ev.position(), cx),
                    },
                ))
            });
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(self.face_picker(code, face, cx))
                .children(size)
        };
        let interface = face(false, faces.ui.as_deref(), cx);
        let code = face(true, faces.code.as_deref(), cx);
        let ui_size = match sized {
            true => size_picker("ui-size", 190., format!("{} %", s.ui_zoom))
                .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                    this.open_ui_sizes(false, ev.position(), cx)
                }))
                .into_any_element(),
            false => ui_zoom.into_any_element(),
        };
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
            setting_row("Interface", "Interface", "", interface, ui_reset.flatten())
                .into_any_element(),
            setting_row(
                "Code",
                "Code",
                match sized {
                    true => "Ligatures on, when the face has them",
                    false => "Grows with the interface size.",
                },
                code,
                code_reset.flatten(),
            )
            .into_any_element(),
            setting_row(
                "Interface size",
                "Interface size",
                size_line,
                ui_size,
                size_reset.flatten(),
            )
            .into_any_element(),
        ]
    }

    /// The code's sizes, in points.
    fn open_code_sizes(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let now = code_points(self.settings.zoom);
        let entries = offered(&CODE_POINTS, now)
            .into_iter()
            .map(|points| {
                Entry::new(
                    format!("{points} pt"),
                    Act::new(move |this, _, cx| this.set_zoom(zoom_for(points), cx)),
                )
                .checked(points == now)
            })
            .collect();
        self.open_titled(at, vec![(None, entries)], cx);
    }

    /// The interface sizes, each named by its text's points (`points`) or its percent.
    fn open_ui_sizes(&mut self, points: bool, at: Point<Pixels>, cx: &mut Context<Self>) {
        let now = self.settings.ui_zoom;
        let entries = offered(&UI_ZOOMS, now)
            .into_iter()
            .map(|zoom| {
                let name = match points {
                    true => format!("{} pt", ui_points(zoom)),
                    false => format!("{zoom} %"),
                };
                Entry::new(
                    name,
                    Act::new(move |this, window, cx| this.set_ui_zoom(zoom, window, cx)),
                )
                .checked(zoom == now)
            })
            .collect();
        self.open_titled(at, vec![(None, entries)], cx);
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
            .h(px(24.))
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

    /// A face's menu: the default, a few installed faces, each in itself; Other… for all, what it opens its tooltip,
    /// with no heading or hint (as the design draws it).
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
                (None, entries),
                (
                    None,
                    vec![
                        Entry::new(
                            "Other…",
                            Act::new(move |this, _, cx| this.open_all_faces(code, cx)),
                        )
                        .tip_two("Other…", "any installed face"),
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

/// A size in use on a 24 px box, a menu's button.
fn size_picker(id: &'static str, wide: f32, label: String) -> gpui::Stateful<gpui::Div> {
    let menu = wide > 100.;
    div()
        .id(id)
        .flex_none()
        .w(px(wide))
        .h(px(24.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(ROW_RADIUS))
        .bg(theme::hover())
        .cursor_pointer()
        .hover(|s| s.bg(theme::selected()))
        .text_size(px(13.))
        .child(
            div()
                .flex_1()
                .when(!menu, |d| d.flex().justify_center())
                .child(label),
        )
        .children(menu.then(|| {
            icons::icon("chevron")
                .size(px(12.))
                .text_color(theme::muted())
        }))
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

    #[test]
    fn a_sizes_menu_holds_the_one_in_use_in_its_place() {
        assert_eq!(ui_points(100), 13);
        assert_eq!(offered(&UI_ZOOMS, 100), UI_ZOOMS);
        assert_eq!(offered(&[80, 100], 90), [80, 90, 100]);
        assert_eq!(offered(&[80, 100], 250), [80, 100, 250]);
    }
}
