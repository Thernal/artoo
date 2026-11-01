//! The title bar, the window's and never the page's (#70): the menu's fold, back and forward, the project (or the
//! tabs, in Tabs mode), search, and on its right the items features put there, then Settings. What a click does is
//! the tab's (`TitleHost`).

use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::{GAP, ROW_RADIUS, icons, keys, px, theme};
use gpui::{
    AnyElement, App, FontWeight, MouseButton, MouseDownEvent, Pixels, Point, Window, div,
    prelude::*,
};
use std::rc::Rc;

/// The title bar's height.
pub const TITLE_BAR: f32 = 38.;

/// What the title bar shows.
pub struct TitleBar {
    /// A repository is open: the menu's fold and search are drawn.
    pub open: bool,
    /// The project's name; None in Tabs mode, where the tabs take its place (D21).
    pub project: Option<String>,
    pub tabs: Option<AnyElement>,
    /// The menu of spaces is folded to icons.
    pub folded: bool,
    /// Whether there is somewhere to go back and forward to.
    pub back: bool,
    pub forward: bool,
    /// Narrow: search is its icon (#80).
    pub compact: bool,
    /// What features and the tab put before Settings: their items, the fetch, the refresh status.
    pub items: Vec<AnyElement>,
}

/// What a click on the title bar does.
pub trait TitleHost {
    fn fold(&self, window: &mut Window, cx: &mut App);
    fn back(&self, window: &mut Window, cx: &mut App);
    fn forward(&self, window: &mut Window, cx: &mut App);
    /// The project's right click, the rail's menu, at `at`.
    fn project_menu(&self, at: Point<Pixels>, window: &mut Window, cx: &mut App);
    /// The project clicked: a repository to open.
    fn projects(&self, window: &mut Window, cx: &mut App);
    fn search(&self, window: &mut Window, cx: &mut App);
    fn settings(&self, window: &mut Window, cx: &mut App);
}

/// The title bar: the project's own quiet words, the fetch and the refresh status before the gear.
pub fn title_bar(bar: TitleBar, host: Rc<dyn TitleHost>) -> impl IntoElement {
    let tabbed = bar.tabs.is_some();
    let (fold, back, forward, menu, projects, search, settings) = (
        host.clone(),
        host.clone(),
        host.clone(),
        host.clone(),
        host.clone(),
        host.clone(),
        host.clone(),
    );
    div()
        .h(px(TITLE_BAR))
        .flex_none()
        .flex()
        .items_center()
        .gap_3()
        .pl(px(artoo_core::platform::TITLEBAR_INSET))
        .pr(px(GAP))
        .child(
            div()
                .flex()
                .gap(px(2.))
                .children(bar.open.then(|| {
                    nav_button(
                        "fold-places",
                        "sidebar",
                        if bar.folded {
                            "Expand the menu  ⌘B"
                        } else {
                            "Collapse the menu  ⌘B"
                        },
                        true,
                    )
                    .on_click(move |_, window, cx| fold.fold(window, cx))
                }))
                .child(
                    nav_button("go-back", "back", "Back  ⌘[", bar.back)
                        .on_click(move |_, window, cx| back.back(window, cx)),
                )
                .child(
                    nav_button("go-forward", "forward", "Forward  ⌘]", bar.forward)
                        .on_click(move |_, window, cx| forward.forward(window, cx)),
                ),
        )
        .children(bar.tabs)
        .children(bar.project.map(|repo| {
            // The repository is a menu: open, refresh and the recent repositories.
            div()
                .id("repo-menu")
                // Its right click, the rail's: name and colour, show, a window, close.
                .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
                    menu.project_menu(e.position, window, cx)
                })
                .flex()
                .items_center()
                .gap_1()
                .h(px(28.))
                .px_2()
                .rounded(px(ROW_RADIUS))
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(repo)
                .child(icons::icon("chevron").text_color(theme::muted()))
                // Open a repository (⌘T): the projects filtered as you type, a path, Browse
                // (the developer, 2026-10-23: its old menu took no typing).
                .on_click(move |_, window, cx| projects.projects(window, cx))
        }))
        .child(
            // The empty part of the bar: a double click zooms the window, as in other apps.
            // Search sits in its middle (at its end beside the tabs) and gives way first when
            // the window is narrow; the buttons on the right stay.
            div()
                .id("title-bar")
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .h_full()
                .flex()
                .items_center()
                .when(tabbed, |s| s.justify_end())
                .when(!tabbed, |s| s.justify_center())
                .on_click(|event, window, _| {
                    if event.click_count() == 2 {
                        window.titlebar_double_click();
                    }
                })
                // Narrow, search is its icon (#80).
                .children((bar.open && bar.compact).then(|| {
                    icon_button("title-search-icon", "search", "Search or run a command  ⌘K")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_, window, cx| search.search(window, cx))
                }))
                .children((bar.open && !bar.compact).then(|| {
                    div()
                        .id("title-search")
                        .flex_shrink(1.)
                        .min_w(px(40.))
                        .w(px(if tabbed { 220. } else { 360. }))
                        .h(px(26.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(10.))
                        .overflow_hidden()
                        .rounded(px(ROW_RADIUS))
                        .border_1()
                        .border_color(theme::island_border())
                        .bg(theme::panel())
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .cursor_pointer()
                        .hover(|s| s.border_color(theme::border()))
                        .child(
                            icons::icon("search")
                                .size(px(14.))
                                .text_color(theme::faint()),
                        )
                        .child(hover::cut("search-hint", "Search or run a command").flex_1())
                        .child(keys::caps("⌘K", keys::On::Plain))
                        // A double click here is not the bar's: the window keeps its size.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_, window, cx| host.search(window, cx))
                })),
        )
        // The right end never gives way.
        .child(
            div()
                .flex_none()
                .min_w(px(if tabbed { 0. } else { 180. }))
                .flex()
                .items_center()
                .justify_end()
                .gap_3()
                .children(bar.items)
                .child(
                    icon_button("settings-open", "settings", "Settings  ⌘,")
                        .on_click(move |_, window, cx| settings.settings(window, cx)),
                ),
        )
}

/// A square icon button for the title bar; the tooltip names it and its shortcut.
pub fn icon_button(
    id: &'static str,
    icon: &'static str,
    tip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .group(id)
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        // An svg takes its colour from itself, not from its parent.
        .child(
            icons::icon(icon)
                .text_color(theme::muted())
                .group_hover(id, |s| s.text_color(theme::text())),
        )
        .tooltip_show_delay(hover::LONG)
        .tooltip(move |_, cx| cx.new(|_| Tip(tip)).into())
}

/// Back / forward: dim when there is nowhere to go (the handlers check again).
pub fn nav_button(
    id: &'static str,
    icon: &'static str,
    tip: &'static str,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let button = div()
        .id(id)
        .size(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .group(id)
        .child(
            icons::icon(icon)
                .text_color(if enabled {
                    theme::muted()
                } else {
                    theme::faint()
                })
                .when(enabled, |s| {
                    s.group_hover(id, |s| s.text_color(theme::text()))
                }),
        );
    if enabled {
        button
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| Tip(tip)).into())
    } else {
        button.opacity(0.45)
    }
}
