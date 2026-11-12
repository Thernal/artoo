//! The agents' contract (#165 stage 4): what another feature asks of the agents' chats — for now the reports a
//! chat drafted of a problem it hit (`report_problem`, #118), which Report a problem opens to be checked — and the
//! models the tab speaks with them: the comments for an agent (`comment`) and the kinds of place they are on (`places`, #212), an agent's notes on its changes
//! (`notes`), what goes over the agents' folder (`bus`); a tab's agents (`tab`), and how they
//! are drawn there (`view`); the kinds of agent, Claude Code's, Codex's and any other offered (`provider`).
//!
//! - `agents::get(cx).take()` is the drafts not opened yet; a worker takes a clone of the `Arc` first.
//! - Each call reads the agents' folder: off the window's thread when it is called often.

pub mod bus;
pub mod comment;
pub mod notes;
pub mod notices;
pub mod places;
pub mod provider;
pub mod tab;
pub mod view;

use gpui::{App, Global};
use std::sync::Arc;

/// A problem an agent's chat hit and drafted a report of: the reviewer checks it and sends it, or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportDraft {
    pub title: String,
    pub description: String,
    /// Who drafted it: "Claude Code · payments-service".
    pub by: String,
}

/// The reports agents drafted.
pub trait ReportDrafts: Send + Sync {
    /// The drafts not taken yet, oldest first. Each is taken once, by the window that asks first.
    fn take(&self) -> Vec<ReportDraft>;
}

struct Installed(Arc<dyn ReportDrafts>);

impl Global for Installed {}

/// Sets the drafts every window reads (the agents' `install`; a test's own).
pub fn provide(cx: &mut App, drafts: Arc<dyn ReportDrafts>) {
    cx.set_global(Installed(drafts));
}

/// The drafts, as provided.
pub fn get(cx: &App) -> Arc<dyn ReportDrafts> {
    cx.global::<Installed>().0.clone()
}
