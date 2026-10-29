//! Artoo — review commits and force-pushed branch versions without an IDE. See README.md.

// A GUI app on Windows: no console window behind it.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod ui;

use artoo_agents::{agents, codex, mcp, notes, review, watch};
use artoo_core::{
    applog, codeowners, editor, emoji, files, fswatch, generated, git, highlight, labels, links,
    markdown, mddiff, platform, rank, search, storage, structural, symbols, textedit, themes,
    worddiff,
};
use artoo_hosts::{avatars, connect, ide, merge, mr, pictures};
use artoo_reports as report;
use artoo_ui_kit::file_icons;
use artoo_updates as updates;
use std::path::PathBuf;

fn main() {
    // An explicit path opens just that folder (a repository or not, #75); without one the window comes back as it was
    // left (the tabs of the last session), or offers to open one.
    // What an agent runs in its chat (`claude mcp add artoo -- <this> mcp`): no window.
    applog::catch_panics();
    match std::env::args().nth(1).as_deref() {
        Some("mcp") => return mcp::run(),
        Some("agent-watch") => return watch::run(),
        _ => {}
    }
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .filter(|p| p.is_dir() || git::Repo::open(p).is_ok());
    ui::run(path);
}

#[cfg(test)]
mod tests {
    /// The core is another crate now (#165): its `cfg!(test)` is false here, so its guard must hold through
    /// the `sandbox` feature the dev-dependencies turn on, or these tests would write the user's data folder.
    #[test]
    fn the_cores_test_guard_holds_in_the_apps_tests() {
        assert!(crate::platform::sandboxed());
    }
}
