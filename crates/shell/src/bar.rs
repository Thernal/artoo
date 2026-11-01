//! The page bar (#71) drawn: one 44 px line over every page — the path ending in the item's id, each part above it a
//! way up; "‹ Inbox" while a page reached from another space is on screen; the page's view switch; its actions. A
//! request's bar carries the public orange along its top. What the path is and what a click does are the tab's.

use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, flat, island, px, theme, words};
use gpui::{
    AnyElement, App, ClickEvent, MouseButton, MouseDownEvent, Pixels, Point, SharedString, Window,
    div, prelude::*,
};
use std::rc::Rc;

/// A part of the path above the page.
pub struct Crumb {
    pub label: String,
    pub mono: bool,
    /// It goes up to its page.
    pub link: bool,
    /// A folder of Code: a click lists what is beside it, a right click its menu (#132, B).
    pub folder: bool,
    /// Its list is open.
    pub open: bool,
    /// A note for the agent waits on it: its dot.
    pub noted: bool,
    /// What is beside it, listed under it.
    pub list: Option<AnyElement>,
}

/// The bar of the page on screen.
pub struct PageBar {
    /// The space a page was reached from: "‹ Inbox".
    pub from: Option<&'static str>,
    /// Narrow: the path is ‹, one level up (#80).
    pub up: bool,
    pub crumbs: Vec<Crumb>,
    /// The page's title, with what its long hover says.
    pub title: AnyElement,
    pub count: Option<String>,
    pub view: Option<AnyElement>,
    pub actions: Vec<AnyElement>,
    /// Public: what is done here is seen on GitLab (#71).
    pub public: bool,
}

/// What a click on the bar does.
pub trait BarHost {
    /// ‹ Inbox: back where the page was reached from.
    fn back(&self, window: &mut Window, cx: &mut App);
    /// ‹: one level up.
    fn up(&self, window: &mut Window, cx: &mut App);
    /// Crumb `n` clicked: up to its page.
    fn up_to(&self, n: usize, window: &mut Window, cx: &mut App);
    /// Folder crumb `n` clicked: what is beside it.
    fn folder(&self, n: usize, window: &mut Window, cx: &mut App);
    /// Folder crumb `n`'s right click, at `at`.
    fn folder_menu(&self, n: usize, at: Point<Pixels>, window: &mut Window, cx: &mut App);
}

/// The bar.
pub fn page_bar(bar: PageBar, host: Rc<dyn BarHost>) -> AnyElement {
    let crumbs = bar.crumbs.into_iter().enumerate().map(|(n, crumb)| {
        let (link, folder, menu) = (host.clone(), host.clone(), host.clone());
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .id(SharedString::from(format!("crumb-{n}")))
                    .relative()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .when(crumb.mono, |s| s.font_family(theme::code_font()))
                    .when(crumb.open, |s| s.text_color(theme::accent()))
                    .child(words::say(&crumb.label).into_owned())
                    .when(crumb.noted, |s| {
                        s.child(div().size(px(6.)).rounded_full().bg(theme::agent()))
                    })
                    .when(crumb.link && !crumb.folder, |s| {
                        s.cursor_pointer()
                            .hover(|s| s.text_color(theme::accent()))
                            .on_click(move |_: &ClickEvent, window, cx| link.up_to(n, window, cx))
                    })
                    .when(crumb.folder, |s| {
                        s.cursor_pointer()
                            .hover(|s| s.text_color(theme::accent()))
                            .on_click(move |_: &ClickEvent, window, cx| {
                                folder.folder(n, window, cx)
                            })
                            .on_mouse_down(
                                MouseButton::Right,
                                move |e: &MouseDownEvent, window, cx| {
                                    menu.folder_menu(n, e.position, window, cx)
                                },
                            )
                    })
                    .children(crumb.list),
            )
            .child(div().text_color(theme::faint()).child("›"))
    });
    let (back, up) = (host.clone(), host.clone());
    let island = island()
        .relative()
        .overflow_hidden()
        .flex_none()
        .flex_row()
        .h(px(44.))
        .items_center()
        .gap(px(12.))
        .pl(px(14.))
        .pr(px(8.))
        .whitespace_nowrap()
        .children(bar.from.map(|from| {
            div()
                .id("bar-from")
                .flex_none()
                .px(px(8.))
                .rounded_full()
                .bg(theme::hover())
                .text_size(px(11.))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.text_color(theme::text()))
                .child(format!("‹ {}", words::say(from)))
                .tooltip_show_delay(hover::LONG)
                .tooltip(|_, cx| cx.new(|_| Tip("Back where you came from  ⌘[")).into())
                .on_click(move |_: &ClickEvent, window, cx| back.back(window, cx))
        }))
        .child(
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .overflow_hidden()
                .text_color(theme::muted())
                .children(bar.up.then(|| {
                    div()
                        .id("crumb-up")
                        .flex_none()
                        .px(px(4.))
                        .rounded(px(ROW_RADIUS))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
                        .child("‹")
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(|_, cx| cx.new(|_| Tip("Up a level  esc")).into())
                        .on_click(move |_: &ClickEvent, window, cx| up.up(window, cx))
                }))
                .children(crumbs)
                .child(bar.title)
                .children(bar.count.map(|n| {
                    div()
                        .flex_none()
                        .ml(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child(n)
                })),
        )
        .children(bar.view)
        .child(div().flex_1().min_w_0())
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(8.))
                .children(bar.actions),
        )
        // Public: what is done here is seen on GitLab (#71).
        // Over the island's own border, its top edge turning down with its round corners (a
        // strip would cross them square).
        .when(bar.public, |s| {
            s.child(
                div()
                    .absolute()
                    .top(px(-1.))
                    .left(px(-1.))
                    .right(px(-1.))
                    .bottom(px(-1.))
                    .rounded(px(if flat() { 0. } else { ISLAND_RADIUS }))
                    .border_t_2()
                    .border_color(theme::warning()),
            )
        });
    island.into_any_element()
}
