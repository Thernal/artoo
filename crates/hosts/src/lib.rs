//! GitLab and GitHub (#165): merge requests and pull requests, the ways to connect a host, merging, people's
//! and comments' pictures. One crate while they reach into one another; stage 3's `Host` trait splits it.

pub mod avatars;
pub mod connect;
pub mod github;
mod hosts;
pub mod ide;
pub mod merge;
pub mod mr;
pub mod pictures;

use artoo_core::{emoji, git, links, platform, search, storage};

use artoo_hosts_api::Hosts;
use std::sync::Arc;

/// Provides the hosts every window reads.
pub fn install(cx: &mut gpui::App) {
    artoo_hosts_api::provide(cx, hosts());
}

/// GitLab and GitHub, for a mode without a window.
pub fn hosts() -> Arc<dyn Hosts> {
    Arc::new(hosts::HostsImpl)
}
