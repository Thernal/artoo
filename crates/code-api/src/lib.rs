//! Code's contract (#75, #165 stage 4): what another feature asks of the Code space in a tab — the project's files
//! at the revision it reads, that revision, the tab a file or a folder opens in, its tabs pinned, closed and stepped
//! through — and the routes of its pages, which anyone opens through the `Frame`.
//!
//! Code offers it in every tab as `Rc<dyn Code>` (`Frame::services`). What it changes is changed at once: ask it
//! from anywhere but Code's own update. What Code asks of the tab in turn (`Host`: the file as the review draws it,
//! the notes on a folder, the files opened lately…) the tab offers there as `Rc<dyn Host>` before its parts are
//! made; a tab without one has no Code.

use artoo_core::git::BranchRef;
use artoo_core::route::Place;
use git2::Oid;
use gpui::{AnyElement, App};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// The Code space, by its id (`artoo_shell_api::spaces`): Code offers it, its pages are in it.
pub const SPACE: &str = "code";

/// Code's scope of search everywhere, by its id (`artoo_shell_api::search`): ⌘P opens ⌘K on it, `@` narrows All to
/// it — the files of the diff on screen first, then the project's.
pub const SEARCH: &str = "files";

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

/// The file Code shows, as the tab has it: being read, read (its head line and what is under it), or none.
pub enum FilePage {
    Reading,
    Read(AnyElement, AnyElement),
    Empty,
}

/// What opens a file in an app (`Host::open_in`).
pub type Opens = Rc<dyn Fn(&mut App)>;

/// What Code asks of its tab, found in the tab's services. What it reads is read while Code draws or answers a click, never
/// while the tab is updating it; what it changes is changed once the update under way is over.
pub trait Host {
    /// File `path` as the tab reads it.
    fn file(&self, path: &str, cx: &mut App) -> FilePage;
    /// The notes for the agent on folder `folder` (how many) and what is drawn of them, the composer with them.
    fn folder_notes(&self, folder: &str, cx: &App) -> (usize, Vec<AnyElement>);
    /// Whether folder `folder` has a note for the agent waiting (#97).
    fn note_waits(&self, folder: &str, cx: &App) -> bool;
    /// The files opened lately, the latest first.
    fn recent_files(&self, cx: &App) -> Vec<String>;
    /// Row `row` of the tree for `path`, fading in under a folder just opened (#110).
    fn tree_row(&self, path: &str, row: AnyElement, cx: &App) -> AnyElement;
    /// The repository's branches, for the revision's menu.
    fn branches(&self, cx: &App) -> Vec<BranchRef>;
    /// Open in the app `path` opens in, when there is one: its menu item's words, and what opens it.
    fn open_in(&self, path: &Path, cx: &App) -> Option<(String, Opens)>;
    /// A note for the agent on folder `folder` started.
    fn note_folder(&self, folder: String, cx: &mut App);
    /// Go to file (⌘P).
    fn go_to_file(&self, cx: &mut App);
    /// File `path` picked in search everywhere: one of the diff on screen (`in_diff`) is picked there, its page on
    /// screen, unless Code is; else it is read in Code, as of the revision the diff shows.
    fn pick_file(&self, path: String, in_diff: bool, cx: &mut App);
    /// Folder `folder` of the tree opened: its rows fade in.
    fn tree_opening(&self, folder: String, cx: &mut App);
    /// Line `line` of the file read brought into view, among the notes the tab pins on its lines (Outline's click).
    fn reveal_line(&self, line: u32, cx: &mut App);
}
