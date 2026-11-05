//! GitLab and GitHub (#165): merge requests and pull requests, the ways to connect a host and its panel, people's and
//! comments' pictures. GitLab's side (`gitlab`) and GitHub's (`github`) implement the hosts' contract
//! (`hosts`), neither reaching into the other; what both use has its own module (`tokens`, `fixture`, `json`).

pub(crate) mod avatars;
mod connect;
mod fixture;
mod github;
mod gitlab;
mod hosts;
mod ide;
mod json;
mod panel;
mod pictures;
mod tokens;

use artoo_core::{emoji, git, links, platform, search, storage};

use artoo_hosts_api::Hosts;
use std::sync::Arc;

/// Offers GitHub's and GitLab's kinds of host, the pictures the kit's markdown draws and the people its avatars show; each
/// tab's connect panel (`artoo_hosts_api::connect`).
pub fn install(cx: &mut gpui::App) {
    for (order, kind) in hosts::kinds() {
        artoo_hosts_api::add_kind(cx, order, kind);
    }
    artoo_ui_kit::pictures::provide(cx, Arc::new(pictures::WebPictures));
    artoo_ui_kit::people::provide(Arc::new(avatars::Avatars));
    artoo_shell_api::add_parts(cx, |frame, _, cx| panel::parts(frame, cx));
}

/// GitLab and GitHub, for a mode without a window.
pub fn hosts() -> Arc<dyn Hosts> {
    Arc::new(artoo_hosts_api::Kinds::new(
        hosts::kinds().into_iter().map(|(_, k)| k).collect(),
    ))
}

/// A test's fixture standing in for both hosts, on its thread (`fixture::TEST_FIXTURE`); `None` takes it back.
#[cfg(any(test, feature = "sandbox"))]
pub fn test_fixture(dir: Option<std::path::PathBuf>) {
    fixture::TEST_FIXTURE.with(|f| *f.borrow_mut() = dir);
}
