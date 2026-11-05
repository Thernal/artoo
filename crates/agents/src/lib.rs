//! Coding agents and Artoo (#165): the chats it knows (a tab's: `chats`, and the reviewer's comments to them:
//! `comments`), the MCP server an agent runs (`mcp`), `agent-watch`, the
//! comments sent to a chat and their answers, notes, Codex; how a chat's badge and the notes are drawn (`view`), an
//! agent's form under its question (`form`),
//! the notes' summary over the diff a piece of the item page (`summary`), the Agents window a tool window
//! (`window`), the card asking which chat a card over the window (`pick`). It reads requests through the hosts' contract (`artoo_hosts_api`), the hosts it is handed.
//! The app reaches it through its contract (`artoo_agents_api`) and the functions below: the wiring, the commands
//! an agent runs, Set up's look at the agents.

mod agents;
mod chats;
mod codex;
mod comments;
mod connect;
mod contract;
mod form;
mod inbox;
mod mcp;
mod notes;
mod pick;
mod review;
mod summary;
mod tab;
mod view;
mod watch;
mod window;

use artoo_core::{applog, generated, git, labels, platform, storage};
use artoo_hosts_api::Hosts;
use std::path::{Path, PathBuf};
use std::sync::Arc;

gpui::actions!(
    artoo,
    [
        /// ⇧⌘A: the Agents window shown or hidden.
        ToggleAgents
    ]
);

/// Wires the agents in: the contract other features read them through (`artoo_agents_api`) — the reports chats
/// drafted, the folder the windows read, how a tab's agents are made and drawn — and their parts in every tab: the notes'
/// summary over the diff, the Agents window, the card asking which chat, Settings → Agents.
pub fn install(cx: &mut gpui::App) {
    artoo_agents_api::provide(cx, Arc::new(contract::ReportDraftsImpl));
    artoo_agents_api::bus::provide(cx, Arc::new(contract::FolderImpl));
    artoo_agents_api::tab::provide(cx, std::rc::Rc::new(tab::make));
    artoo_agents_api::view::provide(cx, std::rc::Rc::new(contract::RendererImpl));
    use artoo_shell_api::{Action, Menu, when};
    artoo_shell_api::add_action(
        cx,
        Action::new("agents.window", ToggleAgents)
            .words("Show or hide the Agents window")
            // Its places in ⌘K and the View menu among Artoo's own, as it always had.
            .order(100)
            // ⇧⌘A as Design gives it; ⌥⌘A sends to an agent (D12, X3).
            .key("cmd-shift-a", when::NOT_TYPING)
            .menu(Menu::View, "Agents")
            .at(0, 4),
    );
    // Between Merge requests and Keyboard, where it always was.
    artoo_shell_api::add_settings_section(
        cx,
        artoo_shell_api::settings::Section {
            id: artoo_agents_api::view::SETTINGS,
            title: "Agents",
            order: 60,
        },
    );
    artoo_shell_api::add_parts(cx, parts);
}

/// What an agent runs in its chat (`claude mcp add artoo -- <this> mcp`): the MCP server, reading requests through
/// `hosts`; no window.
pub fn serve_mcp(hosts: Arc<dyn Hosts>) {
    mcp::run(hosts)
}

/// A chat's watcher (`agent-watch`): its chat's comments handed over, then it exits so the agent wakes.
pub fn watch() {
    watch::run()
}

/// Claude Code's profiles on this Mac.
pub fn claude_profiles() -> Vec<PathBuf> {
    agents::claude_profiles()
}

/// Whether Claude Code profile `profile` has Artoo's MCP server.
pub fn claude_connected(profile: &Path) -> bool {
    agents::claude_connected(profile)
}

/// Whether Codex has Artoo's MCP server.
pub fn codex_connected() -> bool {
    codex::connected()
}

/// Agents connected under GitLance, the name Artoo had.
pub fn connected_as_gitlance() -> bool {
    agents::connected_as_gitlance()
}

/// The command that runs Artoo's MCP server.
pub fn server_command() -> String {
    agents::server_command()
}

/// The command that adds `server` to Codex by hand.
pub fn codex_command(server: &str) -> String {
    codex::add_command(server)
}

/// The parts of a tab that offers its agents' notes, its chats and its comments to them; none in one that does not.
fn parts(
    frame: std::rc::Rc<dyn artoo_shell_api::Frame>,
    _: &mut gpui::Window,
    cx: &mut gpui::App,
) -> artoo_shell_api::Parts {
    let Some(agents) = frame
        .services()
        .get::<std::rc::Rc<dyn artoo_agents_api::tab::Agents>>()
    else {
        return Default::default();
    };
    let Some(tab) = tab::of(&agents) else {
        return Default::default();
    };
    artoo_shell_api::Parts {
        settings: vec![connect::settings(tab.connect.clone(), cx)],
        overlay: Some(pick::overlay(tab.card.clone(), frame.clone(), cx)),
        tool: Some(window::tool(
            tab.chats.clone(),
            tab.comments.clone(),
            frame.clone(),
            cx,
        )),
        pieces: vec![summary::piece(tab.notes.clone(), frame.clone(), cx)],
        actions: vec![(
            Box::new(ToggleAgents),
            std::rc::Rc::new(move |_, cx| frame.toggle_tool(artoo_agents_api::view::WINDOW, cx)),
        )],
        ..Default::default()
    }
}
