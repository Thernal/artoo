//! The Settings page's frame: its sections in the left column, the one on with its heading and Done, its rows fading
//! in (#56, 14). What each section holds is the tab's, and the features' rows (`artoo_shell_api::Section`).

use artoo_ui_kit::{GAP, ROW_RADIUS, island, keys, motion, px, theme, words};
use gpui::{AnyElement, App, ClickEvent, FontWeight, SharedString, Window, div, prelude::*};
use std::rc::Rc;

/// The parts of Settings, in its left column.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Diff,
    Appearance,
    General,
    Repository,
    Requests,
    Agents,
    Keyboard,
}

impl Section {
    pub const ALL: [Section; 7] = [
        Section::Diff,
        Section::Appearance,
        Section::General,
        Section::Repository,
        Section::Requests,
        Section::Agents,
        Section::Keyboard,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Section::Diff => "Diff",
            Section::Appearance => "Appearance",
            Section::General => "General",
            Section::Repository => "Repository",
            Section::Requests => "Merge requests",
            Section::Agents => "Agents",
            Section::Keyboard => "Keyboard",
        }
    }
}

/// The page as the tab sees it.
pub struct Page {
    /// The section on.
    pub on: Section,
    /// Its rows.
    pub content: AnyElement,
    /// The theme editor takes the page's height: its tokens scroll beside a preview that stays.
    pub editing: bool,
}

/// What a click on the frame does.
pub trait SettingsHost {
    /// A section in the left column.
    fn section(&self, section: Section, window: &mut Window, cx: &mut App);
    /// Done: Settings closes.
    fn done(&self, window: &mut Window, cx: &mut App);
}

/// The page.
pub fn page(page: Page, host: Rc<dyn SettingsHost>) -> AnyElement {
    let section = page.on;
    let editing = page.editing;
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
                .pb(px(12.))
                .text_size(px(17.))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Settings"),
        )
        .children(Section::ALL.map(|sec| {
            div()
                .id(sec.label())
                .h(px(30.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .cursor_pointer()
                .when(sec == section, |s| {
                    s.bg(theme::selected()).text_color(theme::text())
                })
                .when(sec != section, |s| s.text_color(theme::muted()))
                .hover(|s| s.bg(theme::hover()))
                .child(words::say(sec.label()).into_owned())
                .on_click({
                    let pick = pick.clone();
                    move |_: &ClickEvent, window, cx| pick.section(sec, window, cx)
                })
        }));
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
                                .max_w(px(if editing { 1180. } else { 760. }))
                                .when(editing, |s| s.flex_1().min_h_0())
                                .mx_auto()
                                .flex()
                                .flex_col()
                                .gap_5()
                                .p(px(24.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .child(div().flex_1().child(heading(
                                            words::say(section.label()).into_owned(),
                                        )))
                                        .child(button("settings-done", "Done", "esc").on_click(
                                            move |_: &ClickEvent, window, cx| done.done(window, cx),
                                        )),
                                )
                                // Another section fades in (#56, 14).
                                .child(motion::appear(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .when(editing, |s| s.flex_1().min_h_0())
                                        .child(page.content),
                                    ("settings-section", section as usize),
                                    motion::FAST,
                                    |content, v| content.opacity(v),
                                )),
                        ),
                ),
        )
        .into_any_element()
}

fn heading(text: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_size(px(15.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.into())
}

/// An on/off switch.
pub fn switch(id: &'static str, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(34.))
        .h(px(20.))
        .rounded_full()
        .flex()
        .items_center()
        .px(px(2.))
        .cursor_pointer()
        .bg(if on { theme::accent() } else { theme::hover() })
        .when(on, |s| s.justify_end())
        .child(div().size(px(16.)).rounded_full().bg(if on {
            theme::base()
        } else {
            theme::muted()
        }))
}

/// A plain button with its key after it: Done, ….
pub fn button(
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
