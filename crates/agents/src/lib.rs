//! Coding agents and Artoo (#165): the chats it knows, the MCP server an agent runs (`mcp`), `agent-watch`, the
//! comments sent to a chat and their answers, notes, Codex. It reads requests through the hosts (`mr`) until
//! stage 3's `Host` trait.

pub mod agents;
pub mod codex;
pub mod mcp;
pub mod notes;
pub mod review;
pub mod watch;

use artoo_core::{applog, generated, git, labels, platform, storage};
