//! Coding agents and Artoo (#165): the chats it knows (a tab's: `chats`, and the reviewer's comments to them:
//! `comments`), the MCP server an agent runs (`mcp`), `agent-watch`, the
//! comments sent to a chat and their answers, notes, Codex; how a chat's badge and the notes are drawn (`view`), an
//! agent's form under its question (`form`),
//! the notes' summary over the diff a piece of the item page (`summary`), the Agents window a tool window
//! (`window`), the card asking which chat a card over the window (`pick`). It reads requests through the hosts' contract (`artoo_hosts_api`), the hosts it is handed.

pub mod agents;
pub mod chats;
pub mod codex;
pub mod comments;
pub mod connect;
mod contract;
pub mod form;
pub mod inbox;
pub mod mcp;
pub mod notes;
pub mod pick;
pub mod review;
pub mod summary;
mod tab;
pub mod view;
pub mod watch;
pub mod window;

use artoo_core::{applog, generated, git, labels, platform, storage};
use std::sync::Arc;

/// Wires the agents in: the contract other features read them through (`artoo_agents_api`) — the reports chats
/// drafted, the folder the windows read, how a tab's agents are made — and their parts in every tab: the notes'
/// summary over the diff, the Agents window, the card asking which chat, Settings → Agents.
pub fn install(cx: &mut gpui::App) {
    artoo_agents_api::provide(cx, Arc::new(contract::ReportDraftsImpl));
    artoo_agents_api::bus::provide(cx, Arc::new(contract::FolderImpl));
    artoo_agents_api::tab::provide(cx, std::rc::Rc::new(tab::make));
    artoo_shell_api::add_parts(cx, parts);
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
        pieces: vec![summary::piece(tab.notes.clone(), frame, cx)],
        ..Default::default()
    }
}
