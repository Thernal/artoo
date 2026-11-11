//! Connecting the agents (Settings → Agents, and Set up's Agents, #143): each kind of agent in its order (#171 phase
//! 5) — Connect for one Artoo connects itself (Claude Code: every profile, off the window's thread) and what came of
//! it, where it was found, and the command that does it by hand; only the command for one connected by hand (Codex).

use crate::agents;
use artoo_agents_api::provider::Provider;
use artoo_shell_api::settings::Rows;
use artoo_ui_kit::{ROW_RADIUS, hover, px, theme};
use gpui::{
    App, ClickEvent, ClipboardItem, Context, Entity, FontWeight, Render, SharedString, Window, div,
    prelude::*,
};
use std::sync::Arc;

/// An agent being connected from one tab, and what came of it.
#[derive(Default)]
pub struct Connect {
    /// Under way.
    pub busy: bool,
    /// What the last one said, and the agent it was.
    pub said: Option<(&'static str, String)>,
}

impl Connect {
    /// Artoo's MCP server added to `provider` (every Claude Code profile), in the background; a chat already open
    /// needs a restart.
    pub fn connect(&mut self, provider: Arc<dyn Provider>, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.said = None;
        cx.notify();
        let name = provider.name();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { provider.connect() })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                this.said = Some((
                    name,
                    match result {
                        Ok(text) => text,
                        Err(text) => format!("Not connected — {text}"),
                    },
                ));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// Settings → Agents, as a part's rows.
pub fn settings(connect: Entity<Connect>, cx: &mut App) -> Rows {
    let view = cx.new(|cx| {
        cx.observe(&connect, |_, _, cx| cx.notify()).detach();
        AgentsSettings { connect }
    });
    Rows::new(artoo_agents_api::view::SETTINGS, 10, view)
}

/// Settings → Agents: each agent's Connect, or its command by hand.
struct AgentsSettings {
    connect: Entity<Connect>,
}

/// An agent's ids' word: its name's first, small ("claude").
fn word(name: &str) -> String {
    name.split_whitespace()
        .next()
        .unwrap_or(name)
        .to_lowercase()
}

impl Render for AgentsSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let server = agents::server_command();
        let (busy, said) = {
            let c = self.connect.read(cx);
            (c.busy, c.said.clone())
        };
        // `tip`: what the label leaves out, on a hover.
        let command = |id: String, label: String, tip: Option<&'static str>, text: String| {
            let copy = text.clone();
            div()
                .flex()
                .flex_col()
                .gap_1()
                .py(px(8.))
                .child(
                    div()
                        .id(SharedString::from(format!("{id}-label")))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(label)
                        .when_some(tip, |d, tip| {
                            d.tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
                        }),
                )
                .child(
                    div()
                        .id(SharedString::from(id))
                        .flex()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_1()
                        .rounded(px(ROW_RADIUS))
                        .bg(theme::base())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .font_family(theme::code_font())
                                .text_size(px(12.))
                                .child(text),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child("Copy"),
                        )
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                        }),
                )
        };
        // One row an agent (#201): what is connected, as facts; Connect all while one is not; the by-hand command
        // with a failure only; an agent connected by hand says its limit on its row.
        let row = |id: String, name: String, facts: String, end: Option<gpui::AnyElement>| {
            div()
                .id(SharedString::from(id))
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
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .child(facts),
                        ),
                )
                .children(end)
        };
        let mut rows = div().flex().flex_col();
        for provider in artoo_agents_api::provider::all(cx) {
            let (name, word) = (provider.name(), word(provider.name()));
            let by_hand = provider.add_command(&server);
            let Some(about) = provider.connects() else {
                // Connected by hand (Codex): its limit on its row, the command to copy while it is not.
                let on = word == "codex" && crate::codex::connected();
                let facts = match (on, provider.limits()) {
                    (true, _) => "connected".to_owned(),
                    (false, Some(_)) => "limited · Only a Codex running in a terminal".to_owned(),
                    (false, None) => "not yet".to_owned(),
                };
                let limits = provider.limits();
                let copy = by_hand.clone();
                let end = (!on).then(|| {
                    div()
                        .id(SharedString::from(format!("agents-copy-{word}")))
                        .px_3()
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(ROW_RADIUS))
                        .border_1()
                        .border_color(theme::island_border())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .child("Copy the command")
                        .when_some(limits, |d, tip| {
                            d.tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
                        })
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                        })
                        .into_any_element()
                });
                rows = rows.child(row(
                    format!("agents-row-{word}"),
                    name.to_owned(),
                    facts,
                    end,
                ));
                continue;
            };
            // Claude Code: each profile, connected or not yet.
            let profiles: Vec<(String, bool)> = if word == "claude" {
                (crate::claude::profiles().iter())
                    .map(|p| {
                        let name = p
                            .file_name()
                            .map(|n| format!("~/{}", n.to_string_lossy()))
                            .unwrap_or_default();
                        (name, crate::claude::connected(p))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let facts = if word != "claude" {
                about.to_owned()
            } else if profiles.is_empty() {
                "No profile found (~/.claude)".to_owned()
            } else {
                (profiles.iter())
                    .map(|(n, on)| format!("{n} {}", if *on { "connected" } else { "not yet" }))
                    .collect::<Vec<_>>()
                    .join(" · ")
            };
            let all_in = !profiles.is_empty() && profiles.iter().all(|(_, on)| *on);
            let to_connect = provider.clone();
            let end = if all_in || (word == "claude" && profiles.is_empty()) {
                None
            } else {
                Some(
                    div()
                        .id(SharedString::from(format!("agents-connect-{word}")))
                        .px_3()
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .rounded(px(ROW_RADIUS))
                        .bg(theme::agent())
                        .text_color(theme::base())
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .child(match (busy, profiles.len()) {
                            (true, _) => "Connecting…",
                            (false, 0 | 1) => "Connect",
                            (false, _) => "Connect all",
                        })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let provider = to_connect.clone();
                            this.connect.update(cx, |c, cx| c.connect(provider, cx))
                        }))
                        .into_any_element(),
                )
            };
            let failed = said
                .as_ref()
                .filter(|(by, text)| *by == name && text.starts_with("Not connected"));
            rows = rows
                .child(row(
                    format!("agents-row-{word}"),
                    name.to_owned(),
                    facts,
                    end,
                ))
                .children(
                    said.as_ref()
                        .filter(|(by, _)| *by == name)
                        .map(|(_, text)| {
                            div()
                                .py(px(6.))
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .child(text.clone())
                        }),
                )
                // The command by hand comes with a failure only.
                .children(failed.map(|_| {
                    command(
                        format!("agents-cmd-{word}"),
                        "By hand, in a terminal:".into(),
                        None,
                        by_hand,
                    )
                }));
        }
        rows
    }
}
