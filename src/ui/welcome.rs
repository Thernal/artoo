//! The window with no repository open (#54): open one (⌘O), the ways the parts offer (connect GitLab or GitHub, the
//! hosts'; clone from a URL, the shell's), each with its card; a folder dropped anywhere opens too. The repositories
//! opened lately are listed under them. The page and the cards' frames are drawn by the shell
//! (`artoo_shell_api::welcome`); the tab hands it the ways, the card open and what opening a folder is.

use super::Workspace;
use artoo_shell_api::welcome;
use gpui::{App, Context, WeakEntity, Window, prelude::*};
use std::path::PathBuf;
use std::rc::Rc;

impl Workspace {
    /// The welcome: Artoo, what it is, the ways in and the drop target — or the card one of them opened —, and the
    /// repositories opened lately.
    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let typing = self.part_typing(cx);
        let mut ways: Vec<welcome::Way> = (self.parts.iter())
            .flat_map(|p| p.ways.iter().cloned())
            .collect();
        ways.sort_by_key(|w| w.order);
        let card = ways.iter().find_map(|w| (w.card)(typing, cx));
        welcome::page(
            welcome::Page {
                ways,
                card,
                recent: self.recent.clone(),
            },
            Rc::new(Glue(cx.weak_entity())),
            self.frame.clone(),
            cx,
        )
    }
}

/// What a click on the welcome does that is the tab's.
struct Glue(WeakEntity<Workspace>);

impl welcome::WelcomeHost for Glue {
    fn open(&self, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| this.open(&super::Open, window, cx))
            .ok();
    }

    fn recent(&self, path: PathBuf, _: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |this, cx| this.request_open(path, None, cx))
            .ok();
    }
}
