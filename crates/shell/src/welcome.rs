//! The window with no repository open drawn (#54): the ways in — open one (⌘O), connect GitLab or GitHub, clone
//! one, drop a folder —, a way's card in their place, and the repositories opened lately under them. What a way does
//! and its card are the tab's.

use artoo_shell_api::welcome::{Page, WelcomeHost, WelcomeRenderer};
use artoo_ui_kit::{GAP, ISLAND_RADIUS, hover, icons, keys, px, section_label, theme};
use gpui::{AnyElement, ClickEvent, Div, FontWeight, div, prelude::*};
use std::rc::Rc;

/// The renderer, installed by `install`.
pub(crate) struct Renderer;

impl WelcomeRenderer for Renderer {
    fn page(&self, page: Page, host: Rc<dyn WelcomeHost>) -> AnyElement {
        self::page(page, host)
    }
}

/// The page.
fn page(page: Page, host: Rc<dyn WelcomeHost>) -> AnyElement {
    let (open, connect, clone) = (host.clone(), host.clone(), host.clone());
    let shows = page.card.is_none();
    let body = match page.card {
        Some(card) => card,
        None => div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                choice(
                    "welcome-open",
                    "folder",
                    "Open a repository",
                    "a folder with a .git inside",
                )
                .child(keys::caps("⌘O", keys::On::Plain))
                .on_click(move |_: &ClickEvent, window, cx| open.open(window, cx)),
            )
            .child(
                choice(
                    "welcome-connect",
                    "pull-request",
                    "Connect GitLab or GitHub",
                    "your review requests and inbox, from a token",
                )
                .on_click(move |_: &ClickEvent, window, cx| connect.card(true, window, cx)),
            )
            .child(
                choice(
                    "welcome-clone",
                    "files",
                    "Clone from a URL",
                    "git clone, then open it",
                )
                .on_click(move |_: &ClickEvent, window, cx| clone.card(false, window, cx)),
            )
            .child(
                div()
                    .h(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.))
                    .border_1()
                    .border_dashed()
                    .border_color(theme::island_border())
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child("or drop a folder anywhere in the window"),
            )
            .into_any_element(),
    };
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .m(px(GAP))
        .mt_0()
        .rounded(px(ISLAND_RADIUS))
        .bg(theme::editor())
        .child(
            div()
                .w(px(560.))
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(px(22.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Artoo"),
                        )
                        .child(div().text_color(theme::muted()).child(
                            "See what changed between the versions of a branch — and review it — on your own machine.",
                        )),
                )
                .child(body)
                .children((!page.recent.is_empty() && shows).then(|| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .mt_2()
                        .child(section_label("Recent"))
                        .children(page.recent.into_iter().enumerate().map(|(ix, path)| {
                            let go = host.clone();
                            div()
                                .id(("recent", ix))
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::hover()))
                                .text_color(theme::muted())
                                .min_w_0()
                                .child(hover::cut("path", path.display().to_string()))
                                .on_click(move |_, window, cx| go.recent(path.clone(), window, cx))
                        }))
                })),
        )
    .into_any_element()
}

/// One of the welcome's ways in: its icon, what it does and a word on it.
fn choice(
    id: &'static str,
    icon: &'static str,
    title: &'static str,
    text: &'static str,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .h(px(56.))
        .px(px(14.))
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(icons::icon(icon).size(px(18.)).text_color(theme::accent()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(title)
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(text),
                ),
        )
}
