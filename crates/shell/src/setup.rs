//! Set up's frame (#143 B): the title and Later, the parts' tabs — a check on a part done — at the top, the part in
//! the column between, which alone scrolls, and the count and Back · Skip · Next at the foot. What each part holds and
//! which parts are offered are the tab's.

use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::discussion::{Kind, button};
use artoo_ui_kit::{GAP, ISLAND_RADIUS, icons, px, theme};
use gpui::{AnyElement, App, ClickEvent, FontWeight, Window, div, prelude::*};
use std::rc::Rc;

/// The page as the tab sees it.
pub struct Page {
    /// Each part's name, and whether it is done as its Settings section says.
    pub tabs: Vec<(&'static str, bool)>,
    /// The part on.
    pub at: usize,
    pub body: AnyElement,
}

/// What a click on the frame does.
pub trait SetupHost {
    /// Part `at`: a tab, Back, Skip, Next.
    fn go(&self, at: usize, window: &mut Window, cx: &mut App);
    /// Later and Done: what was under it comes back.
    fn close(&self, window: &mut Window, cx: &mut App);
}

/// The page's column: the top, the part and the foot line up in it.
fn column() -> gpui::Div {
    div().w_full().max_w(px(760.)).mx_auto().px(px(24.))
}

/// The page.
pub fn page(page: Page, host: Rc<dyn SetupHost>) -> AnyElement {
    let (at, n) = (page.at, page.tabs.len());
    let last = at + 1 == n;
    let tabs = page
        .tabs
        .into_iter()
        .enumerate()
        .fold(group(), |group, (i, (name, done))| {
            let go = host.clone();
            group.child(
                chip(("setup-tab", i), name, i == at)
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .children(done.then(|| {
                        icons::icon("check")
                            .size(px(12.))
                            .text_color(theme::added())
                    }))
                    .on_click(move |_: &ClickEvent, window, cx| go.go(i, window, cx)),
            )
        });
    let back = (at > 0).then(|| {
        let go = host.clone();
        button("setup-back", "Back", None, Kind::Plain)
            .on_click(move |_: &ClickEvent, window, cx| go.go(at - 1, window, cx))
    });
    let next = at + 1;
    let (done, skip, forward, later) = (host.clone(), host.clone(), host.clone(), host);
    let forward: Vec<AnyElement> = if last {
        vec![
            button(
                "setup-done",
                "Done",
                Some("↵"),
                Kind::Primary(theme::accent()),
            )
            .on_click(move |_: &ClickEvent, window, cx| done.close(window, cx))
            .into_any_element(),
        ]
    } else {
        vec![
            button("setup-skip", "Skip", None, Kind::Plain)
                .on_click(move |_: &ClickEvent, window, cx| skip.go(next, window, cx))
                .into_any_element(),
            button(
                "setup-next",
                "Next",
                Some("↵"),
                Kind::Primary(theme::accent()),
            )
            .on_click(move |_: &ClickEvent, window, cx| forward.go(next, window, cx))
            .into_any_element(),
        ]
    };
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .m(px(GAP))
        .mt_0()
        .rounded(px(ISLAND_RADIUS))
        .bg(theme::editor())
        .child(
            div().flex_none().pt(px(24.)).child(
                column()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .mb(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(18.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Set up Artoo"),
                            )
                            .child(
                                button("setup-later", "Later", Some("esc"), Kind::Plain).on_click(
                                    move |_: &ClickEvent, window, cx| later.close(window, cx),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .pb(px(10.))
                            .border_b_1()
                            .border_color(theme::island_border())
                            .child(tabs),
                    ),
            ),
        )
        .child(
            div()
                .id("setup-body")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(column().pt(px(16.)).pb(px(24.)).child(page.body)),
        )
        .child(
            div()
                .flex_none()
                .py(px(12.))
                .border_t_1()
                .border_color(theme::island_border())
                .child(
                    column()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(12.))
                                .text_color(theme::faint())
                                .child(format!("{} of {n}", at + 1)),
                        )
                        .children(back)
                        .children(forward),
                ),
        )
        .into_any_element()
}
