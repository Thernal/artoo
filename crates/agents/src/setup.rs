//! Set up's Agents (#143 B): Settings → Agents at its shortest — Claude Code with its profiles and whether each has
//! Artoo, Connect for them all; Codex, the command that adds it by hand. Offered again once to agents connected under
//! GitLance, the name Artoo had.

use crate::connect::Connect;
use crate::{agents, claude, codex};
use artoo_agents_api::provider::Provider;
use artoo_core::storage;
use artoo_shell_api::setup::Part;
use artoo_shell_api::{Frame, Toast};
use artoo_ui_kit::controls::{choice_row, tag};
use artoo_ui_kit::discussion::{Kind, button};
use artoo_ui_kit::{px, theme};
use gpui::{AnyElement, App, AppContext, ClickEvent, Entity, div, prelude::*};
use std::rc::Rc;

/// Each Claude Code profile (`~/.claude`) and whether Artoo is in it; whether Codex has it — looked at as the page
/// opens, its tab is picked and a connect ends.
#[derive(Default)]
struct Seen {
    profiles: Vec<(String, bool)>,
    codex: bool,
}

impl Seen {
    fn look(&mut self) {
        self.profiles = claude::profiles()
            .iter()
            .map(|p| {
                let name = p
                    .file_name()
                    .map(|n| format!("~/{}", n.to_string_lossy()))
                    .unwrap_or_default();
                (name, claude::connected(p))
            })
            .collect();
        self.codex = codex::connected();
    }
}

/// The part, with the tab's connect.
pub(crate) fn part(connect: Entity<Connect>, frame: Rc<dyn Frame>, cx: &mut App) -> Part {
    let seen = cx.new(|_| Seen::default());
    // A connect ended: whether the profiles have Artoo now.
    let (watched, redraw) = (seen.downgrade(), frame.clone());
    cx.observe(&connect, move |connect, cx| {
        if !connect.read(cx).busy {
            watched.update(cx, |s, _| s.look()).ok();
        }
        redraw.redraw(cx);
    })
    .detach();
    let (done, shown) = (seen.clone(), seen.downgrade());
    Part {
        key: "agents",
        name: "Agents",
        order: 0,
        done: Rc::new(move |cx| {
            let s = done.read(cx);
            s.codex || s.profiles.iter().any(|(_, on)| *on)
        }),
        body: Rc::new(move |_, cx| body(&seen, &connect, &frame, cx)),
        shown: Some(Rc::new(move |cx| {
            shown.update(cx, |s, _| s.look()).ok();
        })),
        again: Some(Rc::new(|_| {
            let again = storage::read("renamed.txt").is_empty() && agents::connected_as_gitlance();
            if again {
                storage::save(
                    "renamed.txt",
                    "agents offered to connect again\n".to_owned(),
                );
            }
            again
        })),
    }
}

/// Claude Code with its profiles, Codex — Settings → Agents' rows.
fn body(
    seen: &Entity<Seen>,
    connect: &Entity<Connect>,
    frame: &Rc<dyn Frame>,
    cx: &mut App,
) -> AnyElement {
    let server = agents::server_command();
    let codex_cmd = codex::Codex.add_command(&server);
    let (busy, said) = {
        let c = connect.read(cx);
        (c.busy, c.said.clone().map(|(_, text)| text))
    };
    let s = seen.read(cx);
    let all_in = !s.profiles.is_empty() && s.profiles.iter().all(|(_, on)| *on);
    let what = if s.profiles.is_empty() {
        "No Claude Code profile found (~/.claude)".to_owned()
    } else {
        s.profiles
            .iter()
            .map(|(name, on)| format!("{name} {}", if *on { "connected" } else { "not yet" }))
            .collect::<Vec<_>>()
            .join(" · ")
    };
    let claude_end: Option<AnyElement> = if all_in {
        Some(tag("Connected", theme::added()).into_any_element())
    } else if s.profiles.is_empty() {
        None
    } else {
        let connect = connect.clone();
        Some(
            button(
                "setup-connect-claude",
                match (busy, s.profiles.len()) {
                    (true, _) => "Connecting…",
                    (false, 1) => "Connect",
                    (false, _) => "Connect all",
                },
                None,
                Kind::Primary(theme::accent()),
            )
            .on_click(move |_: &ClickEvent, _, cx| {
                // The first agent Artoo connects itself (Claude Code).
                let first = artoo_agents_api::provider::all(cx)
                    .into_iter()
                    .find(|p| p.connects().is_some());
                if let Some(provider) = first {
                    connect.update(cx, |c, cx| c.connect(provider, cx));
                }
            })
            .into_any_element(),
        )
    };
    let codex_end: AnyElement = if s.codex {
        tag("Ready", theme::added()).into_any_element()
    } else {
        let frame = frame.clone();
        button("setup-codex-copy", "Copy the command", None, Kind::Plain)
            .on_click(move |_: &ClickEvent, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(codex_cmd.clone()));
                frame.toast(
                    Toast::new("The command is copied: run it in a terminal"),
                    cx,
                );
            })
            .into_any_element()
    };
    let row = |id: &'static str, icon, name: &str, what: String| {
        choice_row(id, None, icon, name.to_owned(), None, what, false, false)
    };
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .flex()
                .flex_col()
                .rounded(px(10.))
                .bg(theme::panel())
                .border_1()
                .border_color(theme::island_border())
                .overflow_hidden()
                .child(row("setup-claude", "bot", "Claude Code", what).children(claude_end))
                .child(div().h(px(1.)).bg(theme::island_border()))
                .child(
                    row(
                        "setup-codex",
                        "terminal",
                        "Codex",
                        if s.codex {
                            "a Codex running in a terminal listens by itself".to_owned()
                        } else {
                            "codex mcp add, in a terminal".to_owned()
                        },
                    )
                    .child(codex_end),
                ),
        )
        .children(said.map(|said| {
            div()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(said)
        }))
        .child(div().text_size(px(12.)).text_color(theme::muted()).child(
            "A comment you write for an agent goes to its chat; its answers come back on the line.",
        ))
        .into_any_element()
}
