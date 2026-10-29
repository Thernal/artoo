//! The pictures the kit draws by their address — an image a comment or a description shows — as a port: the kit
//! names what its drawing needs, hosts provides it (its `install`), so the kit stays on the core and nothing it
//! draws knows a host. Without one provided, an image is drawn as its name.

use gpui::{App, Global};
use std::sync::Arc;

/// A picture ready to draw, and its size when the file says it.
pub type Picture = (Arc<gpui::Image>, Option<(u32, u32)>);

/// Pictures from the web, kept once fetched.
pub trait Pictures: Send + Sync {
    /// The picture at `url`, once it is in; `None` while it is fetched (asking for it) or when it could not be.
    fn image(&self, url: &str) -> Option<Picture>;

    /// An upload a comment of `project` writes as `/uploads/…` (GitLab's), asked with the host's token where
    /// it can be, as `image` otherwise.
    fn upload(&self, project: &str, src: &str) -> Option<Picture>;
}

struct Installed(Arc<dyn Pictures>);

impl Global for Installed {}

/// Sets the pictures every window draws (hosts' `install`).
pub fn provide(cx: &mut App, pictures: Arc<dyn Pictures>) {
    cx.set_global(Installed(pictures));
}

/// The pictures, as provided; `None` before (a test, the kit alone).
pub fn get(cx: &App) -> Option<Arc<dyn Pictures>> {
    cx.try_global::<Installed>().map(|i| i.0.clone())
}
