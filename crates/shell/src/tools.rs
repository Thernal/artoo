//! The right edge (Design `design/shell/`, #49) drawn: an icon stripe that opens one tool window at a time beside the
//! page, and the window's frame — a title with a faint count, buttons in its header, the body, a foot. Which tools a
//! space has, what each shows and what opening one does are the tab's (the contract `artoo_shell_api::tools`).

use artoo_shell_api::tools::{Button, OnClose, OnTool, Place, ToolView, ToolsRenderer};
use artoo_ui_kit::hover::{self, Tip, TipOwned};
use artoo_ui_kit::{ROW_RADIUS, icons, island, keys, motion, px, theme};
use gpui::{AnyElement, ClickEvent, FontWeight, div, prelude::*};

/// The right edge's renderer, installed by `install`.
pub(crate) struct Renderer;

impl ToolsRenderer for Renderer {
    fn stripe(&self, buttons: Vec<Button>, toggle: OnTool) -> AnyElement {
        stripe(buttons, toggle).into_any_element()
    }

    fn window(&self, view: ToolView, place: Place, close: OnClose) -> AnyElement {
        window(view, place, close).into_any_element()
    }
}

/// The stripe of `buttons`.
fn stripe(buttons: Vec<Button>, toggle: OnTool) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(44.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .pt(px(2.))
        .children(buttons.into_iter().map(|mut b| {
            let under = b.under.take();
            let button = button(b, toggle.clone());
            match under {
                None => button.into_any_element(),
                Some(under) => div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(3.))
                    .child(button)
                    .child(under)
                    .into_any_element(),
            }
        }))
}

fn button(b: Button, toggle: OnTool) -> impl IntoElement {
    let Button {
        id,
        icon,
        tip,
        on,
        badge,
        what,
        ..
    } = b;
    div()
        .id(id)
        .relative()
        .size(px(32.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .group(id)
        .cursor_pointer()
        .when(on, |s| s.bg(theme::selected()))
        .hover(|s| s.bg(theme::hover()))
        .child(
            icons::icon(icon)
                .text_color(if on { theme::text() } else { theme::muted() })
                .group_hover(id, |s| s.text_color(theme::text())),
        )
        .children(badge.map(|(text, color)| {
            div()
                .absolute()
                .right(px(1.))
                .bottom(px(1.))
                .min_w(px(14.))
                .h(px(14.))
                .px(px(3.))
                .rounded_full()
                .bg(color)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(9.))
                .font_weight(FontWeight::BOLD)
                .text_color(theme::base())
                .child(text)
        }))
        .tooltip_show_delay(hover::LONG)
        .tooltip(move |_, cx| {
            let tip = keys::text(tip).into_owned();
            let tip = if what.is_empty() {
                tip
            } else {
                format!("{tip}  ·  {what}")
            };
            cx.new(|_| TipOwned(tip.into())).into()
        })
        .on_click(move |_: &ClickEvent, window, cx| toggle(id, window, cx))
}

/// Tool window `view` at its place; `close` hides it (its ✕).
fn window(view: ToolView, place: Place, close: OnClose) -> impl IntoElement {
    let close = div()
        .id("tool-close")
        .size(px(26.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .group("tool-close")
        .cursor_pointer()
        .hover(|s| s.bg(theme::hover()))
        .child(
            icons::icon("x")
                .size(px(15.))
                .text_color(theme::muted())
                .group_hover("tool-close", |s| s.text_color(theme::text())),
        )
        .tooltip_show_delay(hover::LONG)
        .tooltip(|_, cx| cx.new(|_| Tip("Hide  esc")).into())
        .on_click(move |_: &ClickEvent, window, cx| close(window, cx));
    let over = place.over;
    let room = place.width + place.handle_width;
    let pane = div()
        .flex_none()
        .flex()
        .overflow_hidden()
        .when(over, |s| {
            s.absolute()
                .top(px(0.))
                .bottom(px(0.))
                .right(px(0.))
                .shadow_lg()
        })
        .children(place.handle)
        .child(
            island()
                .w(px(place.width))
                .h_full()
                .border_color(place.border)
                .child(
                    div()
                        .flex_none()
                        .h(px(38.))
                        .pl(px(14.))
                        .pr(px(8.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .border_b_1()
                        .border_color(theme::island_border())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_baseline()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex_none()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(view.title),
                                )
                                .children(
                                    view.count
                                        .map(|c| hover::cut("count", c).text_color(theme::faint())),
                                ),
                        )
                        .children(view.actions)
                        .child(close),
                )
                .child(div().flex_1().min_h_0().flex().flex_col().child(view.body))
                .children(
                    view.foot
                        .map(|foot| artoo_ui_kit::controls::tool_foot().child(foot)),
                ),
        );
    // It comes out of the stripe and goes back into it; its content shows once it is half out.
    motion::toward(
        pane,
        "tool-pane",
        place.shown,
        place.recent,
        motion::BASE,
        move |pane, v| {
            pane.max_w(px(room * v))
                .opacity(((v - 0.4) / 0.6).clamp(0., 1.))
        },
    )
}
