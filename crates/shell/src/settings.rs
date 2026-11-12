//! The Settings page's frame: its sections in the left column, the one on with its heading and Done, its rows fading
//! in (#56, 14). The sections are those offered (`artoo_shell_api::add_settings_section`); what each holds its owner
//! draws.

use artoo_shell_api::settings::{Page, SettingsHost, SettingsRenderer};
use artoo_ui_kit::{GAP, ROW_RADIUS, island, keys, motion, px, theme, words};
use gpui::{AnyElement, ClickEvent, FontWeight, SharedString, div, prelude::*};
use std::rc::Rc;

/// The renderer, installed by `install`.
pub(crate) struct Renderer;

impl SettingsRenderer for Renderer {
    fn page(&self, page: Page, host: Rc<dyn SettingsHost>) -> AnyElement {
        self::page(page, host)
    }
}

/// The page: the left column — Settings, the search field, the sections (each with its hits while searching), Done
/// at its foot outside any scroll (#203) — and the section on, or the hits, in a 640 column.
fn page(page: Page, host: Rc<dyn SettingsHost>) -> AnyElement {
    let on = page.on;
    let editing = page.editing;
    let found = page.found;
    // The section on: its title over its rows, its place what the fade is keyed by.
    let (at, title) = page
        .sections
        .iter()
        .enumerate()
        .find(|(_, s)| s.id == on)
        .map_or((0, ""), |(at, s)| (at, s.title));
    let (pick, done) = (host.clone(), host);
    let nav = div()
        .w(px(200.))
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(2.))
        .p(px(12.))
        .border_r_1()
        .border_color(theme::island_border())
        .child(
            div()
                .px(px(10.))
                .pt(px(8.))
                .pb(px(10.))
                .text_size(px(17.))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Settings"),
        )
        .child(div().pb(px(8.)).child(page.search))
        .children(page.sections.iter().map(|sec| {
            let id = sec.id;
            let hits = found
                .as_ref()
                .map(|f| f.iter().find(|(s, _)| *s == id).map_or(0, |(_, n)| *n));
            let here = hits.is_none() && id == on;
            div()
                .id(id)
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .when(here, |s| s.bg(theme::selected()).text_color(theme::text()))
                .when(!here, |s| {
                    s.text_color(if hits == Some(0) {
                        theme::faint()
                    } else {
                        theme::muted()
                    })
                })
                .hover(|s| s.bg(theme::hover()))
                .child(div().flex_1().child(words::say(sec.title).into_owned()))
                .children(hits.filter(|n| *n > 0).map(|n| {
                    div()
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(n.to_string())
                }))
                .on_click({
                    let pick = pick.clone();
                    move |_: &ClickEvent, window, cx| pick.section(id, window, cx)
                })
        }))
        .child(div().flex_1())
        .child(
            div().px(px(4.)).child(
                button("settings-done", "Done", "esc")
                    .on_click(move |_: &ClickEvent, window, cx| done.done(window, cx)),
            ),
        );
    let searching = found.is_some();
    div()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .px(px(GAP))
        .pb(px(GAP))
        .child(
            island()
                .flex_1()
                .flex_row()
                .bg(theme::editor())
                .child(nav)
                .child(
                    div()
                        .id("settings")
                        .flex_1()
                        .min_w_0()
                        .when(!editing, |s| s.overflow_y_scroll())
                        .when(editing, |s| s.flex().flex_col())
                        .child(
                            div()
                                .w_full()
                                .max_w(px(if editing { 1180. } else { COLUMN + 48. }))
                                .when(editing, |s| s.flex_1().min_h_0())
                                .mx_auto()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .p(px(24.))
                                .when(!searching, |s| {
                                    s.child(heading(words::say(title).into_owned()))
                                })
                                // Another section fades in (#56, 14).
                                .child(motion::appear(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .when(editing, |s| s.flex_1().min_h_0())
                                        .child(page.content),
                                    ("settings-section", if searching { usize::MAX } else { at }),
                                    motion::FAST,
                                    |content, v| content.opacity(v),
                                )),
                        ),
                ),
        )
        .into_any_element()
}

/// The column the rows are drawn in (#203).
const COLUMN: f32 = 640.;

fn heading(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

/// A plain button with its key after it: Done, ….
fn button(
    id: &'static str,
    label: &'static str,
    shortcut: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .py_1()
        .rounded(px(8.))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(label)
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(keys::text(shortcut).into_owned()),
        )
}
