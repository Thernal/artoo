//! Connecting the agents (Settings → Agents, and Set up's Agents, #143): each kind of agent in its order (#171 phase
//! 5) — Connect for one Artoo connects itself (Claude Code: every profile, off the window's thread) and what came of
//! it, where it was found, and the command that does it by hand; only the command for one connected by hand (Codex).

use crate::agents;
use artoo_agents_api::provider::Provider;
use artoo_shell_api::settings::Rows;
use artoo_ui_kit::controls::setting;
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
        let mut rows = div().flex().flex_col();
        for provider in artoo_agents_api::provider::all(cx) {
            let (name, word) = (provider.name(), word(provider.name()));
            let by_hand = provider.add_command(&server);
            let Some(about) = provider.connects() else {
                let label = match provider.limits() {
                    Some(_) => format!("{name} (limited):"),
                    None => format!("{name}:"),
                };
                rows = rows.child(command(
                    format!("agents-cmd-{word}"),
                    label,
                    provider.limits(),
                    by_hand,
                ));
                continue;
            };
            let found = provider.found();
            let to_connect = provider.clone();
            let connect = div()
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
                .child(if busy { "Connecting…" } else { "Connect" })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let provider = to_connect.clone();
                    this.connect.update(cx, |c, cx| c.connect(provider, cx))
                }));
            rows = rows
                .child(setting(name, about, connect))
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
                .children(found.map(|text| {
                    div()
                        .py(px(6.))
                        .text_size(px(12.))
                        .text_color(theme::faint())
                        .child(text)
                }))
                .child(command(
                    format!("agents-cmd-{word}"),
                    "Or by hand, in a terminal:".into(),
                    None,
                    by_hand,
                ));
        }
        rows
    }
}
