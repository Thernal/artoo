//! Coding agents and Artoo (#165): the chats it knows (a tab's: `chats`), the MCP server an agent runs (`mcp`), `agent-watch`, the
//! comments sent to a chat and their answers, notes, Codex; how a chat's badge and the notes are drawn (`view`),
//! the notes' summary over the diff a piece of the item page (`summary`). It reads requests through the hosts'
//! contract (`artoo_hosts_api`), the hosts it is handed.

pub mod agents;
pub mod chats;
pub mod codex;
mod contract;
pub mod inbox;
pub mod mcp;
pub mod notes;
pub mod review;
pub mod summary;
pub mod view;
pub mod watch;

use artoo_core::{applog, generated, git, labels, platform, storage};
use std::sync::Arc;

/// Wires the agents in: the contract other features read them through (`artoo_agents_api`), and their parts in
/// every tab — the notes' summary over the diff.
pub fn install(cx: &mut gpui::App) {
    artoo_agents_api::provide(cx, Arc::new(contract::ReportDraftsImpl));
    artoo_shell_api::add_parts(cx, parts);
}

/// The parts of a tab that offers its agents' notes; none in one that does not.
fn parts(
    frame: std::rc::Rc<dyn artoo_shell_api::Frame>,
    _: &mut gpui::Window,
    cx: &mut gpui::App,
) -> artoo_shell_api::Parts {
    let Some(notes) = frame.services().get::<gpui::Entity<notes::AgentNotes>>() else {
        return Default::default();
    };
    artoo_shell_api::Parts {
        pieces: vec![summary::piece(notes, frame, cx)],
        ..Default::default()
    }
}
