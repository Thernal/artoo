//! The tab's side of Code (#75, `artoo-code`): what the tab asks of it (`artoo_code_api::Code`, read from the
//! tab's services), and the tab as Code's host.

use super::Workspace;
use super::nav::Page;
use super::spaces::CODE;
use crate::git::BranchRef;
use artoo_code_api::{FilePage, Host};
use gpui::{AnyElement, App, Context, WeakEntity};
use std::path::Path;
use std::rc::Rc;

impl Workspace {
    /// Code, as the tab asks it (`artoo_code_api`); none in a build without it.
    pub(super) fn code(&self) -> Option<Rc<dyn artoo_code_api::Code>> {
        self.services.get()
    }

    /// Reads the project's files at Code's revision, once per revision (and again with `force`).
    pub(super) fn read_code_files(&mut self, force: bool, cx: &mut Context<Self>) {
        if let Some(code) = self.code() {
            code.read_files(force, cx);
        }
    }

    /// Folder `path` opens as Code's page (#97), unfolded in the tree, in the preview tab.
    pub(super) fn open_folder_page(&mut self, path: String, cx: &mut Context<Self>) {
        if self.code().is_some_and(|c| c.reveal_folder(&path, cx)) {
            self.tree_opening(&path, cx);
        }
        self.go_page(Page::Folder(path).route(), cx);
    }

    /// File `path` opens in Code: as the preview tab (`preview`) the next click replaces, or kept.
    pub(super) fn open_code_file(&mut self, path: String, preview: bool, cx: &mut Context<Self>) {
        if let Some(code) = self.code() {
            code.place_tab(&path, preview, cx);
        }
        self.open_file_view(path, cx);
    }

    /// ⌥⌘P and ⌘K's Pin tab: the tab on screen in Code pinned or unpinned; false when there is
    /// none.
    pub(super) fn pin_shown_tab(&mut self, cx: &mut Context<Self>) -> bool {
        self.nav.space == CODE && self.code().is_some_and(|c| c.pin_shown(cx))
    }

    /// Whether Code's tab on screen is pinned: ⌘K says Unpin.
    pub(super) fn shown_tab_pinned(&self, cx: &App) -> bool {
        self.nav.space == CODE && self.code().is_some_and(|c| c.shown_pinned(cx))
    }

    /// ⌘W in Code: the tab on screen closes.
    pub(super) fn close_code_tab(&mut self, cx: &mut Context<Self>) {
        if let Some(code) = self.code() {
            code.close_shown(cx);
        }
    }

    /// ⌃⇥ and ⌃⇧⇥ in Code: the next or the previous tab, going round.
    pub(super) fn step_code_tab(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(code) = self.code() {
            code.step_tab(forward, cx);
        }
    }
}

/// The tab as its Code space's host.
pub(super) struct CodeHost(pub(super) WeakEntity<Workspace>);

impl CodeHost {
    /// `change` made to the tab once the update under way is over.
    fn later(
        &self,
        cx: &mut App,
        change: impl FnOnce(&mut Workspace, &mut Context<Workspace>) + 'static,
    ) {
        let tab = self.0.clone();
        cx.defer(move |cx| {
            tab.update(cx, change).ok();
        });
    }
}

impl Host for CodeHost {
    fn file(&self, path: &str, cx: &mut App) -> FilePage {
        self.0
            .update(cx, |tab, cx| tab.file_page(path, cx))
            .unwrap_or(FilePage::Empty)
    }

    fn folder_notes(&self, folder: &str, cx: &App) -> (usize, Vec<AnyElement>) {
        let Some(tab) = self.0.upgrade() else {
            return (0, Vec::new());
        };
        let tab = tab.read(cx);
        let prefix = format!("{folder}/");
        let notes: Vec<u64> = tab
            .comments(cx)
            .iter()
            .filter(|c| c.path == prefix && !c.archived)
            .map(|c| c.id)
            .collect();
        let mut drawn: Vec<AnyElement> = notes
            .iter()
            .map(|&id| tab.render_thread(id, 0., self.0.clone(), cx))
            .collect();
        if tab.folder_compose() == Some(folder) {
            drawn.push(tab.render_composer(0., self.0.clone(), cx));
        }
        (notes.len(), drawn)
    }

    fn note_waits(&self, folder: &str, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|tab| tab.read(cx).folder_note_waits(folder, cx))
    }

    fn recent_files(&self, cx: &App) -> Vec<String> {
        let Some(tab) = self.0.upgrade() else {
            return Vec::new();
        };
        tab.read(cx)
            .recents
            .iter()
            .filter_map(|e| match &e.key {
                super::recents::Key::File(path) => Some(path.clone()),
                _ => None,
            })
            .take(8)
            .collect()
    }

    fn tree_row(&self, path: &str, row: AnyElement, cx: &App) -> AnyElement {
        match self.0.upgrade() {
            Some(tab) => tab.read(cx).tree_row(path, row),
            None => row,
        }
    }

    fn branches(&self, cx: &App) -> Vec<BranchRef> {
        self.0
            .upgrade()
            .map(|tab| tab.read(cx).branch_list(cx).to_vec())
            .unwrap_or_default()
    }

    fn open_in(&self, path: &Path, cx: &App) -> Option<(String, artoo_code_api::Opens)> {
        let app = self.0.upgrade()?.read(cx).open_app(path, cx)?;
        let (tab, path, label) = (
            self.0.clone(),
            path.to_owned(),
            format!("Open in {}", app.name()),
        );
        let open = move |cx: &mut App| {
            tab.update(cx, |tab, _| tab.open_path_with(&path, app.clone()))
                .ok();
        };
        Some((label, Rc::new(open)))
    }

    fn note_folder(&self, folder: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.start_folder_comment(&folder, cx));
    }

    fn go_to_file(&self, cx: &mut App) {
        self.later(cx, |tab, cx| tab.search(super::palette::Scope::Files, cx));
    }

    fn tree_opening(&self, folder: String, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.tree_opening(&folder, cx));
    }

    fn reveal_line(&self, line: u32, cx: &mut App) {
        self.later(cx, move |tab, cx| tab.reveal_file_line(line, cx));
    }
}
