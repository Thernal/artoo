//! Code's contract (#75, #165 stage 4): what another feature asks of the Code space in a tab — the project's files
//! at the revision it reads, that revision, the tab a file or a folder opens in, its tabs pinned, closed and stepped
//! through — and the routes of its pages, which anyone opens through the `Frame`.
//!
//! Code offers it in every tab as `Rc<dyn Code>` (`Frame::services`). What it changes is changed at once: ask it
//! from anywhere but Code's own update.

use artoo_core::route::Place;
use git2::Oid;
use gpui::App;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// A file read in Code.
#[derive(Serialize, Deserialize)]
pub struct FileRoute {
    pub path: String,
}

impl Place for FileRoute {
    const KIND: &'static str = "code.file";
}

/// A folder of the project in Code.
#[derive(Serialize, Deserialize)]
pub struct FolderRoute {
    pub path: String,
}

impl Place for FolderRoute {
    const KIND: &'static str = "code.folder";
}

/// The Code space of one tab.
pub trait Code {
    /// The project's files at the revision read, sorted; none until they are read.
    fn files(&self, cx: &App) -> Arc<Vec<String>>;

    /// The revision read, by name: None the working tree, the files on disk.
    fn revision(&self, cx: &App) -> Option<(String, Oid)>;

    /// Code reads at `rev` from now on (None: the working tree); what it shows is read there as it is shown.
    fn follow(&self, rev: Option<(String, Oid)>, cx: &mut App);

    /// The project's files read at the revision, once per revision, and again with `force`.
    fn read_files(&self, force: bool, cx: &mut App);

    /// Project `root` read, a plain folder when `plain`, its pinned tabs kept by `key`; when it is not the one read
    /// (`fresh`), what was read of that one goes.
    fn set_project(&self, root: PathBuf, plain: bool, fresh: bool, key: &str, cx: &mut App);

    /// File `path` given its tab before its page opens: the preview tab (`preview`) the next file opened so
    /// replaces, or one of its own.
    fn place_tab(&self, path: &str, preview: bool, cx: &mut App);

    /// Folder `path` unfolded in the tree and given the preview tab before its page opens (#97): whether it was
    /// folded.
    fn reveal_folder(&self, path: &str, cx: &mut App) -> bool;

    /// ⌥⌘P and ⌘K's Pin tab: the tab on screen pinned or unpinned; false when there is none.
    fn pin_shown(&self, cx: &mut App) -> bool;

    /// Whether the tab on screen is pinned: ⌘K says Unpin.
    fn shown_pinned(&self, cx: &App) -> bool;

    /// ⌘W: the tab on screen closes, its neighbour shows, else the project's page; a pinned one stays (#20).
    fn close_shown(&self, cx: &mut App);

    /// ⌃⇥ and ⌃⇧⇥: the next or the previous tab, going round.
    fn step_tab(&self, forward: bool, cx: &mut App);
}
