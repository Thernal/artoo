//! The kinds of agent Artoo works with (#171 phase 5): Claude Code and Codex, the agents' own, each offered with `add`
//! and another the same way. Each says how a session of it is found — the one this program runs in, its recent chats
//! in a folder —, how it is connected to Artoo's MCP server, and how its sessions listen when they run no watcher of
//! their own. Settings → Agents, the card asking which chat and the folder's beat ask them all, in their order.

use gpui::{App, Global};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The session a program runs in, as its agent tells a command it runs (the MCP server, a chat's watcher).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Me {
    pub session: String,
    /// The agent's process; 0 when it does not say.
    pub pid: u32,
    /// Its config folder, where it keeps one per profile.
    pub config: Option<PathBuf>,
}

/// One kind of agent. Its methods block (they read its files and run its program): off the window's thread when
/// they read more than the environment.
pub trait Provider: Send + Sync {
    /// Its name, as a chat says who it is: "Claude Code". One name per provider.
    fn name(&self) -> &'static str;

    /// The session this program runs in, when it is this agent's: from what the agent puts in a command's
    /// environment.
    fn me(&self) -> Option<Me>;

    /// Session `me`'s first message, to tell chats apart; empty when it cannot be read.
    fn title(&self, _me: &Me) -> String {
        String::new()
    }

    /// What Connect does, when Artoo connects it itself (Settings → Agents, Set up); none when it is connected by hand.
    fn connects(&self) -> Option<&'static str> {
        None
    }

    /// Connects it to Artoo's MCP server: what happened, or why it did not.
    fn connect(&self) -> Result<String, String> {
        Err(format!("{} is connected by hand", self.name()))
    }

    /// Where it was found on this machine, said under Connect ("Profiles: .claude"); none when there is nothing to say.
    fn found(&self) -> Option<String> {
        None
    }

    /// The command that connects it to `server` (this program and its `mcp`), run by hand in a terminal.
    fn add_command(&self, server: &str) -> String;

    /// What it cannot do here, said where it is connected and where a chat of it is picked; none when nothing.
    fn limits(&self) -> Option<&'static str> {
        None
    }

    /// Whether it is connected under GitLance's name only, the name Artoo had: connecting again moves it over.
    fn connected_as_gitlance(&self) -> bool;

    /// Its chats in folder `root` that do not listen, newest first, as (session, title): a comment sent to one waits
    /// until it listens.
    fn chats(&self, _root: &Path) -> Vec<(String, String)> {
        Vec::new()
    }

    /// The program a new chat opens in a terminal, given a prompt; none when it cannot start one.
    fn program(&self) -> Option<&'static str> {
        None
    }

    /// One beat for project `project`: its sessions there that run no watcher of their own listen through the app,
    /// and what was sent to one is handed over. Nothing for one whose chats watch themselves.
    fn beat(&self, _project: &Path) {}
}

#[derive(Default)]
struct Installed(Vec<(u16, Arc<dyn Provider>)>);

impl Global for Installed {}

/// Offers `provider` in `order` among the others (the agents' `install`). A name offered already is refused, and the
/// log says so: the first to offer it keeps it.
pub fn add(cx: &mut App, order: u16, provider: Arc<dyn Provider>) {
    let list = &mut cx.default_global::<Installed>().0;
    if list.iter().any(|(_, p)| p.name() == provider.name()) {
        artoo_core::applog::line(&format!(
            "agent provider {} offered twice: the second refused",
            provider.name()
        ));
        return;
    }
    list.push((order, provider));
    list.sort_by_key(|(order, _)| *order);
}

/// The providers offered, in their order; none before the agents are installed.
pub fn all(cx: &App) -> Vec<Arc<dyn Provider>> {
    cx.try_global::<Installed>()
        .map(|i| i.0.iter().map(|(_, p)| p.clone()).collect())
        .unwrap_or_default()
}

/// The provider named `name` (a chat's `agent`), when it is offered.
pub fn named(cx: &App, name: &str) -> Option<Arc<dyn Provider>> {
    all(cx).into_iter().find(|p| p.name() == name)
}
