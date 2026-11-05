//! The window with no repository open drawn (#54): the ways in — open one (⌘O), the parts' ways (connect GitLab or
//! GitHub, clone one), drop a folder —, a way's card in their place, and the repositories opened lately under them.
//! Opening a folder is the tab's; a way and its card's fields are its owner's, the card's frame drawn here.

use artoo_shell_api::Frame;
use artoo_shell_api::welcome::{Card, Page, Tone, WelcomeHost, WelcomeRenderer};
use artoo_ui_kit::controls::bar_button;
use artoo_ui_kit::{GAP, ISLAND_RADIUS, ROW_RADIUS, hover, icons, keys, px, section_label, theme};
use gpui::{AnyElement, ClickEvent, Div, FontWeight, div, prelude::*};
use std::rc::Rc;

/// The renderer, installed by `install`.
pub(crate) struct Renderer;

impl WelcomeRenderer for Renderer {
    fn page(&self, page: Page, host: Rc<dyn WelcomeHost>, frame: Rc<dyn Frame>) -> AnyElement {
        self::page(page, host, frame)
    }
}

/// The page.
fn page(page: Page, host: Rc<dyn WelcomeHost>, frame: Rc<dyn Frame>) -> AnyElement {
    let open = host.clone();
    let shows = page.card.is_none();
    let body = match page.card {
        Some(card) => self::card(card, &frame),
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
            .children(page.ways.into_iter().map(|way| {
                let go = way.go.clone();
                choice(way.id, way.icon, way.title, way.text)
                    .on_click(move |_: &ClickEvent, window, cx| go(window, cx))
            }))
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

/// A way's card: its title and word, each field under its label (a link beside it), what it said, Back and its
/// button.
fn card(card: Card, frame: &Rc<dyn Frame>) -> AnyElement {
    let label = |text: &'static str| {
        div()
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(text)
    };
    let mut fields = div().flex().flex_col().gap(px(6.));
    for (ix, field) in card.fields.into_iter().enumerate() {
        fields = if ix == 0 && field.link.is_none() {
            fields.child(label(field.label))
        } else {
            fields.child(
                div()
                    .when(ix > 0, |d| d.pt(px(4.)))
                    .flex()
                    .items_center()
                    .child(div().flex_1().child(label(field.label)))
                    .children(field.link.map(|(id, words, press)| {
                        div()
                            .id(id)
                            .text_size(px(12.))
                            .text_color(theme::accent())
                            .cursor_pointer()
                            .child(words)
                            .on_click(move |_: &ClickEvent, window, cx| press(window, cx))
                    })),
            )
        };
        let el = div()
            .id(field.id)
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .rounded(px(ROW_RADIUS))
            .border_2()
            .border_color(if field.active {
                theme::focus()
            } else {
                theme::island_border()
            })
            .bg(theme::base())
            .cursor_text()
            .child(field.inside);
        fields = fields.child(artoo_shell_api::input_box(el, field.input, frame));
    }
    let (back, (go_id, go_label, go)) = (card.back, card.go);
    div()
        .p(px(20.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .rounded(px(ISLAND_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::panel())
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(card.title),
                )
                .children((!card.text.is_empty()).then(|| {
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(card.text)
                })),
        )
        .child(fields)
        .children(card.said.map(|(text, tone)| {
            let color = match tone {
                Tone::Muted => theme::muted(),
                Tone::Good => theme::added(),
                Tone::Bad => theme::removed(),
            };
            div().text_size(px(12.)).text_color(color).child(text)
        }))
        .child(
            div()
                .flex()
                .justify_end()
                .gap_2()
                .child(
                    bar_button("welcome-back", "Back", "esc", false)
                        .on_click(move |_: &ClickEvent, window, cx| back(window, cx)),
                )
                .child(
                    bar_button(go_id, go_label, "↵", true)
                        .on_click(move |_: &ClickEvent, window, cx| go(window, cx)),
                ),
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
