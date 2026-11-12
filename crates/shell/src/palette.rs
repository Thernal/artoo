//! Search everywhere's window drawn (#52): its tabs, each named with what it found, the field, the rows grouped, and
//! the foot with the tab's key. The same window picks the refs of a comparison, a theme, a recent place, a bookmark, a
//! face. What it lists, what the field holds and what a row does are the tab's.

use artoo_shell_api::palette::{Dot, PaletteHost, PaletteRenderer, PaletteView, Tab};
use artoo_ui_kit::{
    ISLAND_RADIUS, ROW_RADIUS, hover, icons, island_label, keys, motion, px, row, theme, words,
};
use gpui::{AnyElement, MouseButton, SharedString, div, prelude::*};
use std::rc::Rc;

/// The renderer, installed by `install`.
pub(crate) struct Renderer;

impl PaletteRenderer for Renderer {
    fn palette(&self, view: PaletteView, host: Rc<dyn PaletteHost>) -> AnyElement {
        self::palette(view, host)
    }
}

/// A dot after a row's words, as the menu's open rows draw theirs: drafts in the agents' colour, news in the accent.
fn dot(n: usize, dot: Dot) -> impl IntoElement {
    let (color, tip) = match dot {
        Dot::Drafts => (theme::agent(), "Unsent drafts"),
        Dot::News => (theme::accent(), "New activity"),
    };
    div()
        .id(("palette-dot", n))
        .flex_none()
        .size(px(7.))
        .rounded_full()
        .bg(color)
        .tooltip_show_delay(hover::LONG)
        .tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
}

/// The window, over everything.
fn palette(view: PaletteView, host: Rc<dyn PaletteHost>) -> AnyElement {
    let mut last = SharedString::default();
    let mut rows = Vec::new();
    for (ix, item) in view.rows.into_iter().enumerate() {
        if item.group != last {
            last = item.group.clone();
            rows.push(
                div()
                    .px(px(10.))
                    .pt(px(8.))
                    .pb(px(2.))
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(words::say(&item.group).to_uppercase())
                    .into_any_element(),
            );
        }
        let selected = ix == view.at;
        let dotted = !item.dots.is_empty();
        let run = host.clone();
        rows.push(
            row(("palette-item", ix), selected)
                .h(px(28.))
                .gap_3()
                .children(item.icon.map(|p| icons::file_icon(&p, 16.)))
                .children(
                    (item.glyph)
                        .map(|name| icons::icon(name).size(px(16.)).text_color(theme::muted())),
                )
                .child(
                    hover::cut("label", keys::text(&item.label).into_owned())
                        // A face is drawn in itself (#24).
                        .when_some(item.face, |s, face| s.font_family(face))
                        .when(!item.file && !dotted, |s| s.flex_1())
                        // Its dots right after its words (#194).
                        .when(!item.file && dotted, |s| s.flex_initial().min_w(px(0.)))
                        .when(item.file, |s| s.flex_none().max_w(px(320.))),
                )
                .children(
                    item.dots
                        .iter()
                        .enumerate()
                        .map(|(k, &d)| dot(ix * 2 + k, d)),
                )
                .children((dotted && !item.file).then(|| div().flex_1()))
                .child(
                    // A file's folder gives way first; a command's key never does.
                    if item.file {
                        hover::cut("detail", item.detail).flex_1()
                    } else {
                        div().id("detail").flex_none().child(item.detail)
                    }
                    .text_size(px(11.))
                    .text_color(theme::muted()),
                )
                .children((!item.key.is_empty()).then(|| keys::caps(&item.key, keys::On::Plain)))
                .on_click(move |_, window, cx| run.run(ix, window, cx))
                .into_any_element(),
        );
    }
    let empty = rows.is_empty().then(|| {
        div()
            .px(px(14.))
            .py(px(12.))
            .text_color(theme::muted())
            .child(view.empty)
    });
    let close = host.clone();
    div()
        .id("palette-backdrop")
        .absolute()
        .size_full()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(72.))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            close.close(window, cx)
        })
        // It fades in as it grows from 98 % (#56, 6).
        .child(motion::appear(
            div()
                .occlude()
                .flex()
                .flex_col()
                .rounded(px(ISLAND_RADIUS))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .shadow_lg()
                .overflow_hidden()
                .children(view.title.map(island_label))
                .children(view.tabs.map(|tabs| scopes(tabs, host.clone())))
                .child(
                    div()
                        .h(px(44.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .children(view.actions.then(|| {
                            div()
                                .flex_none()
                                .px(px(8.))
                                .h(px(22.))
                                .flex()
                                .items_center()
                                .rounded(px(ROW_RADIUS))
                                .bg(theme::info_bg())
                                .text_size(px(12.))
                                .text_color(theme::accent())
                                .child("> Actions")
                        }))
                        .child(view.field),
                )
                .child(div().h(px(1.)).bg(theme::island_border()))
                .child(
                    div()
                        .id("palette-list")
                        .max_h(px(480.))
                        .overflow_y_scroll()
                        .pb(px(6.))
                        .px(px(6.))
                        .children(rows)
                        .children(empty),
                )
                .children(view.foot.map(foot)),
            "palette-coming",
            motion::FAST,
            |panel, v| panel.w(px(motion::mix(627., 640., v))).opacity(v),
        ))
        .into_any_element()
}

/// The tabs of search everywhere: names, and what each found in a badge; one that found
/// nothing fades.
fn scopes(tabs: Vec<Tab>, host: Rc<dyn PaletteHost>) -> AnyElement {
    div()
        .flex()
        .gap(px(2.))
        .px(px(10.))
        .border_b_1()
        .border_color(theme::island_border())
        .children(tabs.into_iter().enumerate().map(|(i, tab)| {
            let (count, lit) = (tab.count, tab.lit);
            let none = count == Some(0);
            let pick = host.clone();
            let tab = div()
                .id(("search-scope", i))
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(10.))
                .pt(px(10.))
                .pb(px(8.))
                .border_b_2()
                .text_size(px(13.))
                .text_color(if lit { theme::text() } else { theme::muted() })
                .when(lit, |s| s.font_weight(gpui::FontWeight::SEMIBOLD))
                .when(none && !lit, |s| s.opacity(0.6))
                .cursor_pointer()
                .child(words::say(tab.label).into_owned())
                .children(count.map(|n| {
                    div()
                        .min_w(px(18.))
                        .px(px(5.))
                        .rounded_full()
                        .flex()
                        .justify_center()
                        .text_size(px(11.))
                        .bg(if lit {
                            theme::info_bg()
                        } else {
                            theme::hover()
                        })
                        .text_color(if lit { theme::accent() } else { theme::faint() })
                        .child(n.to_string())
                }))
                .on_click(move |_, window, cx| pick.scope(i, window, cx));
            // The underline goes from the tab left to the tab come to (#56, 6).
            motion::toward(
                tab,
                ("search-scope-line", i),
                lit,
                false,
                motion::FAST,
                |tab, v| {
                    tab.border_color(gpui::Rgba {
                        a: v,
                        ..theme::accent()
                    })
                },
            )
        }))
        .into_any_element()
}

/// The tab on with its key, and how to move.
fn foot(on: String) -> AnyElement {
    div()
        .flex()
        .gap(px(14.))
        .px(px(14.))
        .py(px(8.))
        .border_t_1()
        .border_color(theme::island_border())
        .text_size(px(11.))
        .text_color(theme::muted())
        .child(on)
        .child("⇥ next tab · > actions")
        .child(div().flex_1())
        .child("↵ open · esc")
        .into_any_element()
}
