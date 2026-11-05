//! Connecting the agents (Settings → Agents, and Set up's Agents, #143): Artoo's MCP server added to every Claude
//! Code profile, off the window's thread, and what came of it; the commands that do it by hand, Claude Code's and
//! Codex's.

use crate::{agents, codex};
use artoo_ui_kit::controls::setting;
use artoo_ui_kit::{ROW_RADIUS, hover, px, theme};
use gpui::{
    AnyView, App, ClickEvent, ClipboardItem, Context, Entity, FontWeight, Render, Window, div,
    prelude::*,
};

/// Claude Code being connected from one tab, and what came of it.
#[derive(Default)]
pub struct Connect {
    /// Under way.
    pub busy: bool,
    /// What the last one said.
    pub said: Option<String>,
}

impl Connect {
    /// Artoo's MCP server added to every Claude Code profile, in the background; a chat already open needs a
    /// restart.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.said = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { agents::connect_claude() })
                .await;
            this.update(cx, |this, cx| {
                this.busy = false;
                this.said = Some(match result {
                    Ok(text) => text,
                    Err(text) => format!("Not connected — {text}"),
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

/// Settings → Agents, as a part's rows.
pub fn settings(connect: Entity<Connect>, cx: &mut App) -> (&'static str, AnyView) {
    let view = cx.new(|cx| {
        cx.observe(&connect, |_, _, cx| cx.notify()).detach();
        AgentsSettings { connect }
    });
    (artoo_agents_api::view::SETTINGS, view.into())
}

/// Settings → Agents: connect Claude Code (every profile), the commands for others.
struct AgentsSettings {
    connect: Entity<Connect>,
}

impl Render for AgentsSettings {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let server = agents::server_command();
        let claude_cmd = format!("claude mcp add -s user artoo -- {server}");
        let codex_cmd = codex::add_command(&server);
        let profiles = agents::claude_profiles()
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .collect::<Vec<_>>()
            .join(", ");
        let (busy, said) = {
            let c = self.connect.read(cx);
            (c.busy, c.said.clone())
        };
        let connect = div()
            .id("agents-connect-claude")
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
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.connect.update(cx, |c, cx| c.connect(cx))
            }));
        // `tip`: what the label leaves out, on a hover.
        let command =
            |id: &'static str, label: &'static str, tip: Option<&'static str>, text: String| {
                let copy = text.clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .py(px(8.))
                    .child(
                        div()
                            .id(gpui::SharedString::from(format!("{id}-label")))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(label)
                            .when_some(tip, |d, tip| {
                                d.tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
                            }),
                    )
                    .child(
                        div()
                            .id(id)
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
        div()
            .flex()
            .flex_col()
            .child(setting(
                "Claude Code",
                "Adds Artoo's MCP server to every Claude Code profile; a chat already open needs a restart.",
                connect,
            ))
            .children(said.map(|text| {
                div()
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(text)
            }))
            .child(
                div()
                    .py(px(6.))
                    .text_size(px(12.))
                    .text_color(theme::faint())
                    .child(if profiles.is_empty() {
                        "No Claude Code profile found.".to_owned()
                    } else {
                        format!("Profiles: {profiles}")
                    }),
            )
            .child(command(
                "agents-cmd-claude",
                "Or by hand, in a terminal:",
                None,
                claude_cmd,
            ))
            .child(command(
                "agents-cmd-codex",
                "Codex (limited):",
                Some(codex::LIMITS),
                codex_cmd,
            ))
    }
}
