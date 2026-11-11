//! Settings → Notifications (#196, Design round 6 M4, round 7 K2): System notifications with where they come — or
//! macOS's refusal and the way to its settings —, every kind of notice by its source with its three switches (the
//! app's sources first, then `From plugins N` and a group per plugin under its name and letter mark), and the Dock's
//! count. The kinds are the registry's (`artoo_shell_api::notices`); what the person turns is kept there.

use super::{Workspace, px, theme};
use artoo_shell_api::notices::{self, Channel, Kind, Source};
use artoo_ui_kit::controls::switch;
use artoo_ui_kit::hover::Hover;
use artoo_ui_kit::{ROW_RADIUS, icons};
use gpui::{AnyElement, ClickEvent, Context, FontWeight, SharedString, div, prelude::*};

/// The section's id and place: after General.
pub(super) const NOTIFICATIONS: &str = "notifications";

/// A column's width, and its head's words and long hover.
const COLUMN: f32 = 72.;
const COLUMNS: [(Channel, &str, &str); 3] = [
    (Channel::Inbox, "Inbox", "Waits until you are done with it"),
    (Channel::Toast, "Toast", "While Artoo is in front"),
    (Channel::System, "System", "While no window is in front"),
];

impl Workspace {
    /// What macOS says of Artoo's notifications, asked again off the window's thread: as the section opens and as
    /// System notifications is turned on.
    pub(super) fn read_notices_allowed(&mut self, cx: &mut Context<Self>) {
        if cfg!(feature = "snapshot") && std::env::var_os("ARTOO_SNAPSHOT").is_some() {
            // A snapshot draws the refusal when asked to; it asks the system nothing.
            if std::env::var_os("ARTOO_SNAPSHOT_NOTICES_REFUSED").is_some() {
                self.notices_allowed = Some(false);
            }
            return;
        }
        cx.spawn(async move |this, cx| {
            let allowed = cx
                .background_executor()
                .spawn(async { crate::platform::notifications_allowed() })
                .await;
            this.update(cx, |this, cx| {
                if this.notices_allowed != allowed {
                    this.notices_allowed = allowed;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn render_notice_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let s = &self.settings;
        let refused = self.notices_allowed == Some(false);
        let system = switch("system-notifications", s.system_notifications).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.set_system_notifications(!this.settings.system_notifications, cx)
            },
        ));
        let line: AnyElement = if refused && s.system_notifications {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(12.))
                .text_color(theme::warning())
                .child("macOS has notifications off for Artoo ·")
                .child(
                    div()
                        .id("open-notification-settings")
                        .cursor_pointer()
                        .text_color(theme::accent())
                        .hover(|s| s.underline())
                        .child("Open System Settings")
                        .on_click(|_: &ClickEvent, _, cx| {
                            cx.open_url(crate::platform::NOTIFICATION_SETTINGS)
                        }),
                )
                .into_any_element()
        } else {
            div()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child("While no Artoo window is in front.")
                .into_any_element()
        };
        let system_row = div()
            .flex()
            .items_center()
            .gap_4()
            .py(px(10.))
            .border_b_1()
            .border_color(theme::island_border())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child("System notifications")
                    .child(line),
            )
            .child(div().flex_none().child(system));

        // System's column is dim while nothing can reach the system.
        let system_off = !s.system_notifications || refused;
        let sources = notices::sources(cx);
        let guests = sources.iter().filter(|s| s.guest).count();
        let mut table = div().flex().flex_col().pt(px(8.)).child(head(system_off));
        let (mut rows, mut said) = (0, false);
        for source in &sources {
            // Before the first plugin's group (the plugins' come last): what follows is theirs.
            if source.guest && !said {
                table = table.child(from_plugins(guests));
                said = true;
            }
            table = table.child(group_row(source));
            for kind in &source.kinds {
                table = table.child(kind_row(kind, rows, system_off, cx));
                rows += 1;
            }
        }

        let dock = switch("dock-count", s.dock_count).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.change_settings(
                    |this| this.settings.dock_count = !this.settings.dock_count,
                    cx,
                );
                this.dock_count(cx);
            },
        ));
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(system_row)
            .child(table)
            .children(crate::platform::MAC.then(|| {
                div()
                    .pt(px(8.))
                    .child(super::settings::item("Count on the Dock icon", "", dock))
            }))
            .into_any_element()
    }

    /// System notifications turned on or off: on, macOS asks for its permission (where it has one) and is asked
    /// whether it gave it.
    pub(super) fn set_system_notifications(&mut self, on: bool, cx: &mut Context<Self>) {
        self.change_settings(|this| this.settings.system_notifications = on, cx);
        if on {
            super::system_notes::identify(cx);
            cx.show_system_notification(gpui::SystemNotification {
                tag: "system-notifications-on".into(),
                title: "Notifications are on".into(),
                body: "Artoo tells you here what comes while it is in the background.".into(),
                actions: Vec::new(),
            });
            self.read_notices_allowed(cx);
        }
    }

    /// Kind `id` on `channel` turned the other way, kept for every tab.
    fn turn_notice(&mut self, id: &'static str, channel: Channel, cx: &mut Context<Self>) {
        let on = notices::on(cx, id, channel);
        notices::set(cx, id, channel, !on);
        // The Inbox and its count follow at once.
        self.dock_count(cx);
        cx.notify();
    }
}

/// The columns' heads, each with its long hover.
fn head(system_off: bool) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .h(px(28.))
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::faint())
        .child(div().flex_1());
    for (channel, words, tip) in COLUMNS {
        row = row.child(
            div()
                .id(SharedString::from(format!("notices-head-{words}")))
                .w(px(COLUMN))
                .flex()
                .justify_center()
                .when(channel == Channel::System && system_off, |s| {
                    s.opacity(0.55)
                })
                .child(words)
                .tip(tip),
        );
    }
    row.into_any_element()
}

/// `From plugins 3`: quiet, over the plugins' groups.
fn from_plugins(n: usize) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .pt(px(20.))
        .pb(px(2.))
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child("FROM PLUGINS"),
        )
        .child(n.to_string())
        .into_any_element()
}

/// A source's group: its name, a plugin's with its letter mark.
fn group_row(source: &Source) -> AnyElement {
    let letter = source
        .name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default();
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(32.))
        .pt(px(12.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text())
        .when(source.guest, |s| {
            s.child(
                div()
                    .flex_none()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .bg(theme::selected())
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme::accent())
                    .child(letter),
            )
        })
        .child(source.name)
        .into_any_element()
}

/// A kind's row: its name, then a box per channel — a dash for the Inbox of a kind with a home of its own.
fn kind_row(kind: &Kind, ix: usize, system_off: bool, cx: &mut Context<Workspace>) -> AnyElement {
    let mut row = div()
        .flex()
        .items_center()
        .h(px(32.))
        .text_size(px(13.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pl(px(16.))
                .text_color(theme::text())
                .child(kind.name),
        );
    for (channel, _, _) in COLUMNS {
        let cell = div().w(px(COLUMN)).flex().justify_center();
        if channel == Channel::Inbox && kind.inbox.is_none() {
            row = row.child(cell.text_color(theme::faint()).child("–"));
            continue;
        }
        let on = notices::is_on(cx, kind.id, channel);
        let dim = channel == Channel::System && system_off;
        let id = kind.id;
        let tick = div()
            .id(SharedString::from(format!("notice-{ix}-{channel:?}")))
            .size(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(ROW_RADIUS - 2.))
            .border_1()
            .border_color(if on { theme::accent() } else { theme::faint() })
            .when(on, |s| s.bg(theme::accent()))
            .children(on.then(|| {
                icons::icon("check")
                    .size(px(12.))
                    .text_color(theme::on_accent())
            }))
            .when(dim, |s| s.opacity(0.4))
            .when(!dim, |s| {
                s.cursor_pointer().on_click(
                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.turn_notice(id, channel, cx)
                    }),
                )
            });
        row = row.child(cell.child(tick));
    }
    row.into_any_element()
}

/// The Dock's count, over every tab (#196): each says its Inbox's unread, the Dock shows their sum.
#[derive(Default)]
struct DockCounts(Vec<(gpui::WeakEntity<Workspace>, usize)>);

impl gpui::Global for DockCounts {}

impl Workspace {
    /// This tab's unread said to the Dock, and the Dock's badge set from every tab's, or taken off.
    pub(super) fn dock_count(&mut self, cx: &mut Context<Self>) {
        let unread = self.inbox_unread(cx);
        let me = cx.weak_entity();
        let on = self.settings.dock_count;
        let counts = cx.default_global::<DockCounts>();
        counts
            .0
            .retain(|(tab, _)| tab.upgrade().is_some() && *tab != me);
        counts.0.push((me, unread));
        let total: usize = counts.0.iter().map(|(_, n)| n).sum();
        crate::platform::dock_badge(if on { total } else { 0 });
    }
}
