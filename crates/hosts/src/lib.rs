//! GitLab and GitHub (#165): merge requests and pull requests, the ways to connect a host, merging, people's
//! and comments' pictures. GitLab's side (`gitlab`) and GitHub's (`github`) implement the hosts' contract
//! (`hosts`), neither reaching into the other; what both use has its own module (`tokens`, `fixture`, `json`).

pub mod avatars;
pub mod connect;
pub mod fixture;
mod github;
mod gitlab;
mod hosts;
pub mod ide;
mod json;
pub mod merge;
pub mod pictures;
pub mod tokens;

use artoo_core::{emoji, git, links, platform, search, storage};

use artoo_hosts_api::Hosts;
use std::sync::Arc;

/// Provides the hosts every window reads, and the pictures the kit's markdown draws.
pub fn install(cx: &mut gpui::App) {
    artoo_hosts_api::provide(cx, hosts());
    artoo_ui_kit::pictures::provide(cx, Arc::new(pictures::WebPictures));
}

/// GitLab and GitHub, for a mode without a window.
pub fn hosts() -> Arc<dyn Hosts> {
    Arc::new(hosts::HostsImpl)
}
