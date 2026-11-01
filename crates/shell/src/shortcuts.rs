//! The keyboard card: every key of the app in one place, grouped by what one is doing (the app gives the groups).
//! `?` opens it; esc or a click closes it. Designed in round 2 of the review (`../Design/review/round-2.html`, A29).

use artoo_shell_api::Hook;
use artoo_shell_api::shortcuts::{Group, Keys, OnPreset, ShortcutsRenderer};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, keys, px, theme, words};
use gpui::{AnyElement, ClickEvent, FontWeight, MouseButton, div, prelude::*};

/// The renderer, installed by `install`.
pub(crate) struct Renderer;

impl ShortcutsRenderer for Renderer {
    fn card(
        &self,
        groups: &'static [Group],
        keys: Keys,
        close: Hook,
        pick: OnPreset,
    ) -> AnyElement {
        self::card(groups, keys, close, pick).into_any_element()
    }
}

/// The card over the window: `close` closes it (a click beside it, Close), `pick` picks a preset.
fn card(
    groups: &'static [Group],
    keys: Keys,
    close: Hook,
    pick: OnPreset,
) -> impl IntoElement + use<> {
    let groups = groups.iter().map(|(title, rows)| {
        div()
            .w(px(344.))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::faint())
                    .child(title.to_uppercase()),
            )
            .children(rows.iter().map(|(keys, what)| {
                div()
                    .flex()
                    .gap_3()
                    .child(
                        div()
                            .w(px(124.))
                            .flex_none()
                            .font_family(theme::code_font())
                            .text_size(px(12.))
                            .text_color(theme::accent())
                            .child(keys::text(keys).into_owned()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(theme::muted())
                            .child(words::say(what).into_owned()),
                    )
            }))
    });
    let shut = close.clone();
    div()
        .id("shortcuts-backdrop")
        .absolute()
        .size_full()
        .flex()
        .justify_center()
        .items_start()
        .pt(px(72.))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| shut(cx))
        .child(
            div()
                .occlude()
                .w(px(760.))
                // Taller than the window, the groups scroll; the switch stays in view.
                .max_h(gpui::relative(0.86))
                .flex()
                .flex_col()
                .gap_3()
                .p_4()
                .rounded(px(ISLAND_RADIUS))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .shadow_lg()
                .text_size(px(13.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Keyboard"),
                        )
                        .child(
                            div()
                                .id("shortcuts-close")
                                .px_2()
                                .rounded(px(ROW_RADIUS))
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::hover()))
                                .text_color(theme::muted())
                                .child("Close  esc")
                                .on_click(move |_: &ClickEvent, _, cx| close(cx)),
                        ),
                )
                .child(presets(keys.presets, pick))
                .children(changed_keys(keys.changes))
                .child(
                    div()
                        .id("shortcut-groups")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(24.))
                        .gap_y(px(16.))
                        .children(groups),
                )
                .child(launch_switch()),
        )
}

/// Which keys yours start from — Artoo's, Android Studio's or VS Code's (#54): chosen here when the card greets the
/// first launch, as Settings → Keyboard does later.
fn presets(presets: Vec<(&'static str, bool)>, pick: OnPreset) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child("Your keys start from")
        .child(
            artoo_ui_kit::controls::group().children(presets.into_iter().enumerate().map(
                |(i, (label, on))| {
                    let pick = pick.clone();
                    artoo_ui_kit::controls::chip(("card-preset", i), label, on)
                        .on_click(move |_: &ClickEvent, _, cx| pick(i, cx))
                },
            )),
        )
        .child(
            div()
                .text_color(theme::faint())
                .child("· Settings → Keyboard changes any of them"),
        )
}

/// The keys changed in Settings → Keyboard, above the shipped ones the card lists.
fn changed_keys(changes: Vec<(String, Vec<String>)>) -> Option<impl IntoElement + use<>> {
    (!changes.is_empty()).then(|| {
        div()
            .px_3()
            .py_2()
            .rounded(px(ROW_RADIUS))
            .bg(theme::hover())
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(format!(
                "Changed by you: {} — the list below shows the shipped keys.",
                changes
                    .iter()
                    .map(|(label, keys)| {
                        if keys.is_empty() {
                            format!("{label} (no key)")
                        } else {
                            format!("{label} {}", keys::text(&keys.join(" ")))
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" · ")
            ))
    })
}

/// How to bring the card back.
fn launch_switch() -> impl IntoElement + use<> {
    div()
        .pt(px(8.))
        .border_t_1()
        .border_color(theme::island_border())
        .text_size(px(12.))
        .text_color(theme::faint())
        .child(keys::text("? or ⌘/ shows this again, and Help → Keyboard Shortcuts").into_owned())
}
