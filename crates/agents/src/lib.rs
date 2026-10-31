//! Coding agents and Artoo (#165): the chats it knows, the MCP server an agent runs (`mcp`), `agent-watch`, the
//! comments sent to a chat and their answers, notes, Codex; how a chat's badge and the notes are drawn (`view`). It reads requests through the hosts' contract
//! (`artoo_hosts_api`), the hosts it is handed.

pub mod agents;
pub mod codex;
mod contract;
pub mod mcp;
pub mod notes;
pub mod review;
pub mod view;
pub mod watch;

use artoo_core::{applog, generated, git, labels, platform, storage};
use std::sync::Arc;

/// Wires the agents in: the contract other features read them through (`artoo_agents_api`).
pub fn install(cx: &mut gpui::App) {
    artoo_agents_api::provide(cx, Arc::new(contract::ReportDraftsImpl));
}
