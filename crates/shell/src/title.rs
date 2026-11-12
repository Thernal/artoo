//! The title bar, the window's and never the page's (#70): the menu's fold, back and forward, the project (or the
//! tabs, in Tabs mode), search, and on its right the items features put there, then Settings. What a click does is
//! the tab's (`TitleHost`; the contract `artoo_shell_api::title`).

use artoo_shell_api::title::{TitleBar, TitleHost, TitleRenderer};
use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::{GAP, ROW_RADIUS, icons, keys, px, theme};
use gpui::{AnyElement, App, FontWeight, MouseButton, MouseDownEvent, Window, div, prelude::*};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// The title bar's height.
const TITLE_BAR: f32 = 38.;

/// How long ‹ is held before its menu opens (#194).
const HOLD: Duration = Duration::from_millis(450);

thread_local! {
    /// The press of ‹ the menu waits on: a release or a new press moves it on, so the wait opens nothing.
    static PRESS: Cell<u64> = const { Cell::new(0) };
    /// The menu opened from a hold: the release after it does not go back.
    static HELD: Cell<bool> = const { Cell::new(false) };
}

/// ‹ held: once `HOLD` passes with it still down, its menu of the last places opens at `at`.
fn hold_back(
    host: Rc<dyn TitleHost>,
    at: gpui::Point<gpui::Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let press = PRESS.with(|p| {
        p.set(p.get() + 1);
        p.get()
    });
    HELD.with(|h| h.set(false));
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(HOLD).await;
            if PRESS.with(Cell::get) == press {
                HELD.with(|h| h.set(true));
                cx.update(|window, cx| host.back_menu(at, window, cx)).ok();
            }
        })
        .detach();
}

/// ‹ let go: a hold that has not opened the menu yet opens nothing.
fn let_go() {
    PRESS.with(|p| p.set(p.get() + 1));
}

/// The title bar's renderer, installed by `install`.
pub(crate) struct Renderer;

impl TitleRenderer for Renderer {
    fn render(&self, bar: TitleBar, host: Rc<dyn TitleHost>) -> AnyElement {
        title_bar(bar, host).into_any_element()
    }
}

/// The title bar: the project's own quiet words, the fetch and the refresh status before the gear.
fn title_bar(bar: TitleBar, host: Rc<dyn TitleHost>) -> impl IntoElement {
    let tabbed = bar.tabs.is_some();
    let can_back = bar.back;
    let (fold, back, back_menu, forward, menu, projects, search, settings) = (
        host.clone(),
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
                            "Expand the menu  ⌘B".into()
                        } else {
                            "Collapse the menu  ⌘B".into()
                        },
                        true,
                    )
                    .on_click(move |_, window, cx| fold.fold(window, cx))
                    .relative()
                    .children(keys::held_cap("⌘B", keys::Held::Cmd))
                }))
                .child(
                    // Its tip says where it goes, by name (#194).
                    nav_button(
                        "go-back",
                        "back",
                        format!("{}  ⌘[", bar.back_to.as_deref().unwrap_or("Back")).into(),
                        bar.back,
                    )
                    // A click goes back; held, its menu opens and the release does not (#194).
                    .on_click(move |_, window, cx| {
                        if !HELD.with(|h| h.replace(false)) {
                            back.back(window, cx)
                        }
                    })
                    .on_mouse_down(MouseButton::Left, {
                        let host = back_menu.clone();
                        move |e: &MouseDownEvent, window, cx| {
                            if can_back {
                                hold_back(host.clone(), e.position, window, cx)
                            }
                        }
                    })
                    .on_mouse_up(MouseButton::Left, |_, _, _| let_go())
                    .on_mouse_up_out(MouseButton::Left, |_, _, _| let_go())
                    // Its right click: the last places behind.
                    .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
                        back_menu.back_menu(e.position, window, cx)
                    })
                    .relative()
                    .children(keys::held_cap("⌘[", keys::Held::Cmd)),
                )
                .child(
                    nav_button("go-forward", "forward", "Forward  ⌘]".into(), bar.forward)
                        .on_click(move |_, window, cx| forward.forward(window, cx))
                        .relative()
                        .children(keys::held_cap("⌘]", keys::Held::Cmd)),
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
                // Search's icon at least: the status beside it gives way first (#186).
                .min_w(px(if bar.open { 36. } else { 0. }))
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
                    icon_button("title-search-icon", "search", "Search or run an action  ⌘K")
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
                        .child(hover::cut("search-hint", "Search or run an action").flex_1())
                        .child(keys::caps("⌘K", keys::On::Plain))
                        // A double click here is not the bar's: the window keeps its size.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(move |_, window, cx| host.search(window, cx))
                })),
        )
        // The right end: its items give way (the fetch status cut, #186), the gear never does.
        .child(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .flex()
                .items_center()
                .justify_end()
                .gap_3()
                .child(
                    div()
                        .flex_shrink(1.)
                        .min_w(px(if tabbed { 0. } else { 140. }))
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap_3()
                        .children(bar.items),
                )
                .child(
                    icon_button("settings-open", "settings", "Settings  ⌘,")
                        .flex_none()
                        .on_click(move |_, window, cx| settings.settings(window, cx)),
                ),
        )
}

/// A square icon button for the title bar; the tooltip names it and its shortcut.
fn icon_button(
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
fn nav_button(
    id: &'static str,
    icon: &'static str,
    tip: gpui::SharedString,
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
            .tooltip(move |_, cx| cx.new(|_| hover::TipOwned(tip.clone())).into())
    } else {
        button.opacity(0.45)
    }
}
