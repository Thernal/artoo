//! Background fetch: on for every repository until turned off for it. It runs `git fetch` through
//! `git::fetch` when the window gains focus and every few minutes; the auto-refresh then sees the
//! moved refs and marks new versions.

use super::hover::CutInTip;
use super::px;
use super::{Workspace, format, theme};
use crate::storage;
use gpui::{ClickEvent, Context, Task, div, prelude::*};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INTERVAL: Duration = Duration::from_secs(300);
/// A focus gain fetches only when the last fetch is older than this.
const FOCUS_AFTER: i64 = 60;

#[derive(Default)]
pub struct Fetch {
    pub enabled: bool,
    running: bool,
    /// Seconds since the epoch of the last successful fetch.
    last: Option<i64>,
    error: Option<String>,
    task: Option<Task<()>>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Workspace {
    /// Called when a repository opens: reads whether it is turned off and starts the timer.
    pub(super) fn start_fetching(&mut self, cx: &mut Context<Self>) {
        // A plain folder, or a repository with no remote, has nothing to fetch.
        let enabled = !self.plain
            && self.root.as_deref().is_some_and(|root| {
                storage::fetch_enabled(root)
                    && crate::git::Repo::open(root)
                        .ok()
                        .and_then(|r| r.web_remote_name())
                        .is_some()
            });
        self.fetch = Fetch {
            enabled,
            ..Fetch::default()
        };
        if enabled {
            self.fetch.task = Some(cx.spawn(async move |this, cx| {
                loop {
                    if this.update(cx, |this, cx| this.fetch_now(cx)).is_err() {
                        return;
                    }
                    cx.background_executor().timer(INTERVAL).await;
                }
            }));
        }
        cx.notify();
    }

    pub(super) fn set_fetch_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(root) = &self.root {
            storage::set_fetch_enabled(root, on);
        }
        self.start_fetching(cx);
    }

    /// The window came back to the front.
    pub(super) fn window_activated(&mut self, cx: &mut Context<Self>) {
        self.load_notices(cx);
        if self.fetch.enabled && self.fetch.last.is_none_or(|t| now() - t > FOCUS_AFTER) {
            self.fetch_now(cx);
        }
    }

    pub(super) fn fetch_now(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if !self.fetch.enabled || self.fetch.running {
            return;
        }
        self.fetch.running = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::git::fetch::all(&root) })
                .await;
            this.update(cx, |this, cx| {
                this.fetch.running = false;
                match result {
                    Ok(()) => {
                        this.fetch.last = Some(now());
                        this.fetch.error = None;
                    }
                    Err(e) => this.fetch.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The title bar's word on the fetch, as a fact (#202): when it last ran; a failure an amber dot and
    /// `Can't reach origin · fetched 14:32` (any other `Fetch failed · fetched 14:32`), Retry its own small button.
    /// Its tip keeps the host, the guess and the last fetch.
    /// The fetch status's right click (#205): Retry, Stop fetching this repository.
    fn fetch_menu(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        use super::menu::{Act, Entry};
        let entries = vec![
            Entry::new("Retry", Act::new(|this, _, cx| this.fetch_now(cx))),
            Entry::new(
                "Stop fetching this repository",
                Act::new(|this, _, cx| this.set_fetch_enabled(false, cx)),
            ),
        ];
        self.open_menu(at, vec![entries], cx);
    }

    pub(super) fn render_fetch(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        if !self.fetch.enabled {
            return None;
        }
        let failed = self.fetch.error.as_deref();
        let fetched = self
            .fetch
            .last
            .map(|t| format!("fetched {}", fetched_at(t)));
        let text = match (failed, self.fetch.running, self.fetch.last) {
            (Some(e), _, _) => {
                let head = if unreachable(e) {
                    "Can't reach origin"
                } else {
                    "Fetch failed"
                };
                match (&fetched, self.compact()) {
                    (Some(f), false) => format!("{head} · {f}"),
                    _ => head.to_owned(),
                }
            }
            (None, true, _) => "Fetching…".to_owned(),
            (None, false, Some(t)) => format!("Fetched {}", format::ago(t)),
            (None, false, None) => "Fetch waiting".to_owned(),
        };
        // A long hover: the host, the guess, the last fetch.
        let mut lines: Vec<String> = Vec::new();
        if let Some(w) = &self.web {
            lines.push(format!("origin · {}", w.base));
        }
        if let Some(e) = failed {
            lines.push(e.to_owned());
        }
        if let Some(t) = self.fetch.last {
            lines.push(format!("Last fetched {}", super::hover::full_date(t)));
        }
        lines.push("A click fetches now".into());
        let tip: gpui::SharedString = lines.join("\n").into();
        let status = div()
            .id("fetch-status")
            // It gives way before search and the gear, cut with an ellipsis (#186).
            .flex_shrink(1.)
            .min_w_0()
            .max_w(px(420.))
            .h(px(24.))
            .flex()
            .items_center()
            .gap(px(6.))
            .px_2()
            .rounded(px(super::ROW_RADIUS))
            .text_size(px(11.))
            .text_color(theme::muted())
            .cursor_pointer()
            .hover(|s| s.bg(theme::hover()))
            .tooltip_show_delay(super::hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| super::TipOwned(tip.clone())).into())
            .children(failed.map(|_| {
                div()
                    .flex_none()
                    .size(px(7.))
                    .rounded_full()
                    .bg(theme::warning())
            }))
            .child(div().min_w_0().cut_in_tip().child(text))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.fetch_now(cx)))
            // Where fetching annoys you, it stops (#205).
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, ev: &gpui::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.fetch_menu(ev.position, cx);
                }),
            );
        let retry = failed.map(|_| {
            div()
                .id("fetch-retry")
                .flex_none()
                .h(px(20.))
                .px(px(6.))
                .flex()
                .items_center()
                .rounded(px(5.))
                .border_1()
                .border_color(theme::island_border())
                .text_size(px(11.))
                .text_color(theme::text())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child("Retry")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.fetch_now(cx)))
        });
        Some(
            div()
                .flex_shrink(1.)
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(status)
                .children(retry),
        )
    }
}

/// A failure that is the remote not answering: the network, the VPN.
fn unreachable(error: &str) -> bool {
    error.contains("reach") || error.contains("did not answer")
}

/// When the last fetch was: its time today, else its date and time.
fn fetched_at(t: i64) -> String {
    if format::date(t) == format::date(now()) {
        format::clock(t)
    } else {
        format!("{} {}", format::date(t), format::clock(t))
    }
}
