//! Set up's Agents (#143 B): Settings → Agents at its shortest — Claude Code with its profiles and whether each has
//! Artoo, Connect for them all; Codex, the command that adds it by hand. Offered again once to agents connected under
//! GitLance, the name Artoo had.

use crate::connect::Connect;
use crate::connect::state;
use crate::{agents, claude, codex};
use artoo_agents_api::provider::Provider;
use artoo_core::storage;
use artoo_shell_api::setup::Part;
use artoo_shell_api::{Frame, Toast};
use artoo_ui_kit::discussion::{Kind, button};
use artoo_ui_kit::hover::{self, Hover as _};
use artoo_ui_kit::{icons, px, theme};
use gpui::{AnyElement, App, AppContext, ClickEvent, Entity, FontWeight, div, prelude::*};
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
            .map(|p| (crate::connect::profile_name(p), claude::connected(p)))
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
    let claude_end: Option<AnyElement> = if all_in {
        Some(state(
            "setup-claude-state",
            "circle-check",
            "Connected: listens with Artoo agent-watch".into(),
        ))
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
        state(
            "setup-codex-state",
            "circle-check",
            "Ready: a Codex running in a terminal listens by itself".into(),
        )
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
    // Claude Code's profiles under its name: a glyph and the folder, cut in its middle, its sentence the tooltip.
    let profiles = div()
        .flex()
        .items_center()
        .gap(px(12.))
        .min_w_0()
        .text_size(px(12.))
        .text_color(theme::muted())
        .children(s.profiles.iter().enumerate().map(|(ix, (path, on))| {
            div()
                .id(("setup-profile", ix))
                .flex()
                .items_center()
                .gap(px(4.))
                .min_w_0()
                // A short folder keeps all of its name; a long one gives way, in its middle.
                .when(path.chars().count() <= 24, |d| d.flex_none())
                .child(
                    icons::icon(if *on { "circle-check" } else { "circle-dashed" })
                        .size(px(13.))
                        .flex_none()
                        .text_color(if *on { theme::added() } else { theme::faint() }),
                )
                .child(
                    hover::name(("setup-profile-name", ix), path.clone())
                        .font_family(theme::code_font()),
                )
                .tip(format!(
                    "{path}: {}",
                    if *on {
                        "connected, Artoo’s MCP server in its settings"
                    } else {
                        "not connected yet"
                    }
                ))
        }))
        .when(s.profiles.is_empty(), |d| {
            d.child("No Claude Code profile found (~/.claude)")
        });
    let row =
        |id: &'static str, icon, name: &str, under: Option<AnyElement>, end: Option<AnyElement>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(14.))
                .py(px(10.))
                .child(
                    icons::icon(icon)
                        .flex_none()
                        .size(px(18.))
                        .text_color(theme::muted()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(name.to_owned()),
                        )
                        .children(under),
                )
                .children(end)
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
                .child(row(
                    "setup-claude",
                    "bot",
                    "Claude Code",
                    Some(profiles.into_any_element()),
                    claude_end,
                ))
                .child(div().h(px(1.)).bg(theme::island_border()))
                .child(row(
                    "setup-codex",
                    "terminal",
                    "Codex",
                    None,
                    Some(codex_end),
                )),
        )
        .children(said.map(|said| {
            div()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(said)
        }))
        .into_any_element()
}
