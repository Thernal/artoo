//! GitLance's last word (1.3.1): it is now Artoo. A card over the first window as GitLance starts,
//! until Artoo has run on this computer (its data folder is there): Get Artoo opens Artoo's
//! repository, Later (esc, or a click beside the card) closes it until the next start.

use super::{ISLAND_RADIUS, Workspace, icons, keys, px, theme, updates};
use gpui::{ClickEvent, Context, FontWeight, MouseButton, div, prelude::*};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Artoo's repository: its page says how to install it, and Artoo shows its own updates after.
const ARTOO: &str = "https://github.com/Thernal/artoo";
/// Where the button goes, as the card says it.
const SHOWN_ADDRESS: &str = "github.com/Thernal/artoo";

/// The card is offered once a run, by the first window.
static OFFERED: AtomicBool = AtomicBool::new(false);

/// Whether the first window opens with the card: once a run, until Artoo has run here; a snapshot
/// draws it only when asked (`GITLANCE_SNAPSHOT_VIEW=farewell`).
pub(super) fn due() -> bool {
    if cfg!(feature = "snapshot") && std::env::var_os("GITLANCE_SNAPSHOT").is_some() {
        return false;
    }
    before_artoo(crate::platform::artoo_dir().as_deref()) && !OFFERED.swap(true, Ordering::Relaxed)
}

/// Artoo has not run here yet: its data folder is not there, or there is no knowing where it would be.
fn before_artoo(artoo: Option<&Path>) -> bool {
    !artoo.is_some_and(Path::is_dir)
}

impl Workspace {
    pub(super) fn open_farewell(&mut self, cx: &mut Context<Self>) {
        self.farewell = true;
        cx.notify();
    }

    pub(super) fn close_farewell(&mut self, cx: &mut Context<Self>) {
        self.farewell = false;
        cx.notify();
    }

    /// The card, drawn as the new version's card is: the new name, two lines, Later and Download.
    pub(super) fn render_farewell(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.farewell {
            return None;
        }
        Some(
            div()
                .id("farewell-backdrop")
                .absolute()
                .size_full()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(72.))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.close_farewell(cx)),
                )
                .child(
                    div()
                        .occlude()
                        .w(px(480.))
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
                                .flex_col()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .text_size(px(15.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("GitLance is now Artoo"),
                                )
                                .child(
                                    div()
                                        .text_color(theme::muted())
                                        .child("The same app under a new name; its updates come as Artoo."),
                                )
                                .child(div().text_color(theme::muted()).child(
                                    "Artoo's first start brings over your tokens, review marks, settings and keys; GitLance stays as it is.",
                                )),
                        )
                        .child(
                            div()
                                .pt(px(10.))
                                .border_t_1()
                                .border_color(theme::island_border())
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex_1()
                                        .text_size(px(12.))
                                        .text_color(theme::faint())
                                        .child(SHOWN_ADDRESS),
                                )
                                .child(
                                    updates::button("farewell-later", "Later", false)
                                        .child(keys::caps("esc", keys::On::Plain))
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.close_farewell(cx)
                                        })),
                                )
                                .child(
                                    updates::button("farewell-get", "Get Artoo", true)
                                        .child(
                                            icons::icon("external")
                                                .size(px(13.))
                                                .text_color(theme::base()),
                                        )
                                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                            cx.open_url(ARTOO);
                                            this.close_farewell(cx);
                                        })),
                                ),
                        ),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_waits_only_until_artoo_has_run_here() {
        let home = tempfile::tempdir().unwrap();
        let artoo = home.path().join("Artoo");
        assert!(
            before_artoo(Some(&artoo)),
            "no Artoo folder: the card shows"
        );
        assert!(before_artoo(None), "no home to look in: the card shows");
        std::fs::create_dir(&artoo).unwrap();
        assert!(!before_artoo(Some(&artoo)), "Artoo has run here: no card");
    }

    #[test]
    fn the_button_opens_artoos_repository_as_the_card_says() {
        assert_eq!(ARTOO, "https://github.com/Thernal/artoo");
        assert_eq!(ARTOO, format!("https://{SHOWN_ADDRESS}"));
    }
}
