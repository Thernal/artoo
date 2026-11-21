//! Plugins from private hosts (#179): the hosts Artoo holds a token for, and a read-only search of their repositories
//! with a topic. Nothing here writes to a host: it lists which hosts are connected and what a search finds on one.
//!
//! - `team::get(cx)` is what searches; the hosts' crate provides it (`provide`), the plugins' page asks it.
//! - Every call blocks (curl): off the window's thread.

use anyhow::Result;
use gpui::{App, Global};
use std::sync::Arc;

/// A host Artoo holds a token for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeamHost {
    /// Without the scheme: `github.com`, `gitlab.payments-platform-eng.example.com`.
    pub host: String,
    /// Whose token it is, by username (`you`); empty when not known.
    pub who: String,
    /// What it holds: `Private repositories` on GitHub, `Projects and their releases` on GitLab.
    pub holds: String,
}

/// A repository the topic was found on.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TeamRepo {
    /// `owner/name` (GitLab: `group/project`).
    pub full_name: String,
    /// Its name alone.
    pub name: String,
    pub owner: String,
    pub description: String,
    pub stars: u64,
    pub url: String,
    pub license: String,
    /// Its last push, seconds since the epoch; 0 unknown.
    pub pushed: i64,
    /// Visible only to those with access.
    pub private: bool,
}

/// What can be read of the hosts held.
pub trait Team: Send + Sync {
    /// The hosts a token is held for, as known now: nothing is asked of a host.
    fn hosts(&self) -> Vec<TeamHost>;

    /// The repositories `host`'s token can see that carry `topic`: a read.
    fn search(&self, host: &str, topic: &str) -> Result<Vec<TeamRepo>>;
}

#[derive(Default)]
struct Provided(Option<Arc<dyn Team>>);

impl Global for Provided {}

/// Offers what searches the hosts (the hosts' feature's `install`).
pub fn provide(cx: &mut App, team: Arc<dyn Team>) {
    cx.default_global::<Provided>().0 = Some(team);
}

/// What searches the hosts; none when no feature offered it.
pub fn get(cx: &App) -> Option<Arc<dyn Team>> {
    cx.try_global::<Provided>().and_then(|p| p.0.clone())
}
