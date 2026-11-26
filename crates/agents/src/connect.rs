//! Connecting the agents (Settings → Agents, and Set up's Agents, #143): each kind of agent in its order (#171 phase
//! 5) — Connect for one Artoo connects itself (Claude Code: every profile, off the window's thread) and what came of
//! it, where it was found, and the command that does it by hand; only the command for one connected by hand (Codex).

use crate::agents;
use crate::claude::Link;
use artoo_agents_api::provider::Provider;
use artoo_shell_api::settings::Rows;
use artoo_ui_kit::hover::Hover as _;
use artoo_ui_kit::{ROW_RADIUS, controls, hover, icons, px, theme};
use gpui::{
    App, ClickEvent, ClipboardItem, Context, Entity, Render, SharedString, Window, div, prelude::*,
};
use std::rc::Rc;
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
    let view = |only: Option<&'static str>, cx: &mut App| {
        let connect = connect.clone();
        cx.new(|cx| {
            cx.observe(&connect, |_, _, cx| cx.notify()).detach();
            AgentsSettings {
                connect,
                only,
                folded: Vec::new(),
            }
        })
    };
    let all = view(None, cx);
    // Each agent alone, as Settings' search shows it (#203).
    let (claude, codex) = (view(Some("Claude Code"), cx), view(Some("Codex"), cx));
    Rows::new(artoo_agents_api::view::SETTINGS, 10, all)
        .hit("Claude Code", claude)
        .hit("Codex", codex)
}

/// Settings → Agents: each agent's Connect, or its command by hand.
struct AgentsSettings {
    connect: Entity<Connect>,
    /// The one agent drawn, by its name (Settings' search); every one when none.
    only: Option<&'static str>,
    /// The agents whose commands by hand are folded (the owner, 2026-11-26: always there, folding allowed).
    folded: Vec<String>,
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
        // The command that does it by hand, on one mono line: `>_`, the command, its copy icon.
        let command = |id: String, text: String| {
            let copy = text.clone();
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(28.))
                .pl(px(8.))
                .pr(px(2.))
                .rounded(px(ROW_RADIUS))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::base())
                .child(
                    icons::icon("terminal")
                        .flex_none()
                        .size(px(13.))
                        .text_color(theme::faint()),
                )
                .child(
                    hover::name(SharedString::from(format!("{id}-text")), text)
                        .flex_1()
                        .min_w_0()
                        .font_family(theme::code_font())
                        .text_size(px(12.)),
                )
                .child(
                    icon_button(id, "copy", "Copy the command").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()))
                    }),
                )
        };
        // The reason it failed, in red with its warning glyph, over the command.
        let failure = |why: String, id: String, text: String| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .pl(px(32.))
                .pr(px(12.))
                .pt(px(2.))
                .pb(px(10.))
                .border_b_1()
                .border_color(theme::island_border())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(theme::removed())
                        .child(
                            icons::icon("triangle-alert")
                                .flex_none()
                                .size(px(14.))
                                .text_color(theme::removed()),
                        )
                        .child(
                            hover::cut(SharedString::from(format!("{id}-why")), why)
                                .flex_1()
                                .min_w_0(),
                        ),
                )
                .child(command(id, text))
        };
        // The commands that connect it by hand, always under the agent (the owner, 2026-11-26: not only once it
        // failed), folding at their head.
        let by_hand_block = |word: String,
                             commands: Vec<(String, String)>,
                             folded: bool,
                             cx: &mut Context<Self>| {
            let fold_word = word.clone();
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .pl(px(32.))
                .pr(px(2.))
                .pb(px(if folded { 4. } else { 10. }))
                .border_b_1()
                .border_color(theme::island_border())
                .child(
                    div()
                        .id(SharedString::from(format!("agents-by-hand-{word}")))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .h(px(24.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::text()))
                        .child(
                            icons::icon(if folded {
                                "chevron-right"
                            } else {
                                "chevron-down"
                            })
                            .flex_none()
                            .size(px(13.))
                            .text_color(theme::faint()),
                        )
                        .child(if commands.len() == 1 {
                            "The command, by hand".to_owned()
                        } else {
                            format!("The commands, by hand · {}", commands.len())
                        })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            match this.folded.iter().position(|w| *w == fold_word) {
                                Some(at) => {
                                    this.folded.remove(at);
                                }
                                None => this.folded.push(fold_word.clone()),
                            }
                            cx.notify();
                        })),
                )
                .when(!folded, |s| {
                    s.children(commands.into_iter().map(|(id, text)| command(id, text)))
                })
        };
        // One row an agent (#201), on one line: its name, then its state as a glyph with its sentence as tooltip, and
        // what can be done — Connect all while one is not (the developer, 2026-11-14: the way to connect stays in
        // sight), the rest icons. Framed as every row of Settings is (#203): lit, ringed, walked by ⇥, what was
        // searched marked.
        let row = |id: String, name: String, facts: String, end: Vec<gpui::AnyElement>| {
            let named = SharedString::from(name.clone());
            let why = controls::found_by(&name, &format!("{name} {facts}"));
            controls::setting_frame(&named, div().id(SharedString::from(id)))
                .flex()
                .items_center()
                .gap_3()
                .min_h(px(44.))
                .py(px(5.))
                .border_b_1()
                .border_color(theme::island_border())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().child(controls::found_marked(name)))
                        .children(why.map(|word| {
                            div()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(controls::found_marked(word))
                        })),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .children(end),
                )
        };
        let mut rows = div().flex().flex_col();
        for provider in artoo_agents_api::provider::all(cx) {
            if self.only.is_some_and(|only| only != provider.name()) {
                continue;
            }
            let (name, word) = (provider.name(), word(provider.name()));
            let by_hand = provider.add_command(&server);
            let Some(about) = provider.connects() else {
                // Connected by hand (Codex): its limit on its row, its command to copy.
                let on = word == "codex" && crate::codex::connected();
                let facts = match (on, provider.limits()) {
                    (true, _) => "connected".to_owned(),
                    (false, Some(_)) => "limited · Only a Codex running in a terminal".to_owned(),
                    (false, None) => "not yet".to_owned(),
                };
                // Connected: the ok glyph; else a half ring (limited) or the dashed one, its sentence the tooltip, and
                // Look again, which reads whether Codex has it now; its command to copy is under the row.
                let mut end = Vec::new();
                if on {
                    end.push(state(
                        "agents-state-codex",
                        "circle-check",
                        "Connected: a Codex running in a terminal listens by itself".into(),
                    ));
                } else {
                    end.push(state(
                        "agents-state-codex",
                        match provider.limits() {
                            Some(_) => "circle-half",
                            None => "circle-dashed",
                        },
                        match provider.limits() {
                            Some(_) => {
                                "Not found: a Codex session open here listens by itself".into()
                            }
                            None => "Not connected yet".into(),
                        },
                    ));
                    end.push(
                        text_button("agents-look-codex", "Look again")
                            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.notify()))
                            .into_any_element(),
                    );
                }
                rows = rows.child(row(
                    format!("agents-row-{word}"),
                    name.to_owned(),
                    facts,
                    end,
                ));
                let folded = self.folded.contains(&word);
                rows = rows.child(by_hand_block(
                    word.clone(),
                    vec![(format!("agents-cmd-{word}-hand"), by_hand.clone())],
                    folded,
                    cx,
                ));
                continue;
            };
            // Claude Code: each profile, connected or not yet.
            let profiles: Vec<Profile> = if word == "claude" {
                (crate::claude::profiles().iter())
                    .map(|p| {
                        let (state, server, skill) = crate::claude::link(p);
                        Profile {
                            path: profile_name(p),
                            folder: p
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            link: state,
                            server,
                            skill,
                        }
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
                    .map(|p| {
                        let word = match p.link {
                            Link::Whole => "connected",
                            Link::Partly => "partly",
                            Link::Not => "not yet",
                        };
                        format!("{} {word}", p.path)
                    })
                    .collect::<Vec<_>>()
                    .join(" · ")
            };
            let all_in = !profiles.is_empty() && profiles.iter().all(|p| p.link == Link::Whole);
            let to_connect = provider.clone();
            let none = word == "claude" && profiles.is_empty();
            // Space on the keyboard's row connects it (#203), again once it is.
            if !none {
                let (connect, provider) = (self.connect.clone(), provider.clone());
                controls::setting_keys(
                    name,
                    Rc::new(move |key, _, cx| {
                        key == controls::SettingKey::Space && {
                            connect.update(cx, |c, cx| c.connect(provider.clone(), cx));
                            true
                        }
                    }),
                );
            }
            // Which profile the last failure names (`<folder>: why`); a failure naming none is under the agent.
            let failed = said
                .as_ref()
                .filter(|(by, _)| *by == name)
                .and_then(|(_, text)| text.strip_prefix("Not connected — "));
            let failed_in = failed.and_then(|why| {
                (profiles.iter())
                    .position(|p| why.starts_with(&format!("{}: ", p.folder)))
                    .map(|ix| {
                        let why = why[profiles[ix].folder.len() + 2..].to_owned();
                        (ix, why)
                    })
            });
            let mut end = Vec::new();
            if !none {
                if all_in {
                    // Connected: the ok glyph where there are no rows of profiles, and the same act, quiet, for a
                    // profile added since or Artoo moved.
                    if profiles.is_empty() {
                        end.push(state(
                            "agents-state-claude",
                            "circle-check",
                            "Connected: listens with Artoo agent-watch".into(),
                        ));
                    }
                    end.push(
                        icon_button(
                            format!("agents-connect-{word}"),
                            "rotate-ccw",
                            "Connect again",
                        )
                        .on_click(connect_click(self.connect.clone(), to_connect.clone()))
                        .into_any_element(),
                    );
                } else {
                    end.push(
                        text_button(
                            format!("agents-connect-{word}"),
                            match (busy, profiles.len()) {
                                (true, _) => "Connecting…",
                                (false, 0 | 1) => "Connect",
                                (false, _) => "Connect all",
                            },
                        )
                        .on_click(connect_click(self.connect.clone(), to_connect.clone()))
                        .into_any_element(),
                    );
                }
            }
            rows = rows.child(row(
                format!("agents-row-{word}"),
                name.to_owned(),
                facts,
                end,
            ));
            // One row a profile: its folder, its glyph, Connect again on the one that failed and its reason under it.
            for (ix, p) in profiles.iter().enumerate() {
                let (glyph, tip) = profile_state(&p.path, p.link, p.server, p.skill);
                let failure_here = failed_in.as_ref().filter(|(at, _)| *at == ix);
                rows = rows.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .min_h(px(28.))
                        .pl(px(32.))
                        .pr(px(2.))
                        .py(px(2.))
                        .when(failure_here.is_none(), |d| {
                            d.border_b_1().border_color(theme::island_border())
                        })
                        .child(
                            hover::name(("agents-profile-path", ix), p.path.clone())
                                .flex_1()
                                .min_w_0()
                                .font_family(theme::code_font())
                                .text_size(px(12.)),
                        )
                        .child(
                            div()
                                .w(px(32.))
                                .flex_none()
                                .flex()
                                .justify_center()
                                .child(state(format!("agents-state-claude-{ix}"), glyph, tip)),
                        )
                        .child(div().w(px(104.)).flex_none().flex().justify_end().children(
                            failure_here.map(|_| {
                                text_button(
                                    format!("agents-again-{ix}"),
                                    match busy {
                                        true => "Connecting…",
                                        false => "Connect again",
                                    },
                                )
                                .on_click(connect_click(self.connect.clone(), to_connect.clone()))
                            }),
                        )),
                );
                if let Some((_, why)) = failure_here {
                    let command_here = match p.folder == ".claude" {
                        true => by_hand.clone(),
                        false => format!("CLAUDE_CONFIG_DIR={} {by_hand}", p.path),
                    };
                    rows = rows.child(failure(
                        format!("Not connected — {why}"),
                        format!("agents-cmd-{word}"),
                        command_here,
                    ));
                }
            }
            // What was said of the agent as a whole: that it connected, or a failure naming no profile.
            if failed_in.is_none() {
                if let Some(why) = failed {
                    rows = rows.child(failure(
                        format!("Not connected — {why}"),
                        format!("agents-cmd-{word}"),
                        by_hand.clone(),
                    ));
                } else if let Some((_, text)) = said.as_ref().filter(|(by, _)| *by == name) {
                    rows = rows.child(
                        div()
                            .py(px(6.))
                            .text_size(px(12.))
                            .text_color(theme::muted())
                            .child(text.clone()),
                    );
                }
            }
            // Each profile's command by hand (the one a failure shows already is not said twice); with no profile
            // found, the plain one.
            let mut commands: Vec<(String, String)> = (profiles.iter().enumerate())
                .filter(|(ix, _)| !failed_in.as_ref().is_some_and(|(at, _)| at == ix))
                .map(|(ix, p)| {
                    let text = match p.folder == ".claude" {
                        true => by_hand.clone(),
                        false => format!("CLAUDE_CONFIG_DIR={} {by_hand}", p.path),
                    };
                    (format!("agents-cmd-{word}-{ix}"), text)
                })
                .collect();
            if profiles.is_empty() && failed.is_none() {
                commands.push((format!("agents-cmd-{word}-hand"), by_hand.clone()));
            }
            if !commands.is_empty() {
                let folded = self.folded.contains(&word);
                rows = rows.child(by_hand_block(word.clone(), commands, folded, cx));
            }
        }
        rows
    }
}

/// A click that connects `provider`: Connect all's and a failed profile's Connect again.
fn connect_click(
    connect: Entity<Connect>,
    provider: Arc<dyn Provider>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, _, cx| connect.update(cx, |c, cx| c.connect(provider.clone(), cx))
}

/// A Claude Code profile as its row draws it.
struct Profile {
    /// Its folder as shown (`~/.claude-work`).
    path: String,
    /// Its folder's name, as a failure names it.
    folder: String,
    link: Link,
    server: bool,
    skill: bool,
}

/// A profile's folder as it is shown: under the home folder, `~/…`, the whole of it (cut in its middle where the room
/// is short, `hover::name`).
/// A profile's glyph and tooltip: one glyph for connected, partly, not; the tooltip names the parts.
pub(crate) fn profile_state(
    path: &str,
    link: Link,
    server: bool,
    skill: bool,
) -> (&'static str, String) {
    let yes = |on: bool| if on { "yes" } else { "not yet" };
    match link {
        Link::Whole => ("circle-check", format!("{path}: connected")),
        Link::Not => ("circle-dashed", format!("{path}: not connected yet")),
        Link::Partly => (
            "circle-half",
            format!(
                "Partly connected: Server {} · Skill {}",
                yes(server),
                yes(skill)
            ),
        ),
    }
}

pub(crate) fn profile_name(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// A state as a glyph, its sentence the tooltip: the ok circle in the added colour, the half ring muted, the dashed
/// ring faint.
pub(crate) fn state(
    id: impl Into<SharedString>,
    glyph: &'static str,
    tip: String,
) -> gpui::AnyElement {
    let ok = glyph == "circle-check";
    let el = div().id(id.into()).flex_none().flex().items_center().child(
        icons::icon(glyph).size(px(16.)).text_color(match glyph {
            _ if ok => theme::added(),
            "circle-half" => theme::muted(),
            _ => theme::faint(),
        }),
    );
    // "Title: the sentence" is drawn as the design draws it: a title and, under it, a quieter line.
    match tip.split_once(": ") {
        Some((title, line)) => el.tip_two(title.to_owned(), line.to_owned()),
        None => el.tip(tip),
    }
    .into_any_element()
}

/// A 24 px icon button with its box (the edge and fill of the text button beside it), its sentence the tooltip.
pub(crate) fn icon_button(
    id: impl Into<SharedString>,
    glyph: &'static str,
    tip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .flex_none()
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::hover())
        .cursor_pointer()
        .child(icons::icon(glyph).size(px(13.)).text_color(theme::muted()))
        .tip(tip)
}

/// A 24 px text button, as the settings rows have them.
pub(crate) fn text_button(
    id: impl Into<SharedString>,
    label: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.into())
        .flex_none()
        .h(px(24.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(ROW_RADIUS))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::hover())
        .text_size(px(12.))
        .cursor_pointer()
        .child(label)
}

#[cfg(test)]
mod state_tests {
    use super::*;

    #[test]
    fn each_state_has_its_glyph_and_names_its_parts() {
        let whole = profile_state(".claude", Link::Whole, true, true);
        assert_eq!(whole, ("circle-check", ".claude: connected".to_owned()));
        let not = profile_state(".claude", Link::Not, false, false);
        assert_eq!(not.0, "circle-dashed");
        let partly = profile_state(".claude-work", Link::Partly, true, false);
        assert_eq!(partly.0, "circle-half");
        assert_eq!(partly.1, "Partly connected: Server yes · Skill not yet");
    }
}
