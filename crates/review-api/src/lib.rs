//! The review's contract (#165 stage 4): what a page asks of the tab's review. What a diff is of (`Source`): a
//! commit, a run of commits, two versions, an interdiff, the working tree, a comparison — the review reads it, a page
//! opening one through the review's handle (`Review`), which draws the diff's column with what the shell and the
//! other features put on it (`ColumnLook`). The file Code reads (#71, #75): read at a revision, which one is read, closed. The review draws it; the page puts it
//! where it goes. What the review's host draws on its rows and does with their events (`Host`, `LineEvent`,
//! `FileEvent`, `PaneEvent`, `ChangesEvent`). And how far your review of a merge request got (`Seen`), which the requests list groups by.
//!
//! The tab's review offers it as `Rc<dyn FileView>` in the tab's services (`Frame::services`). What it changes is
//! changed at once: ask it from anywhere but the review's own update.

pub mod diff;

use artoo_core::git::{CommitInfo, PairCommit, Version};
use artoo_core::rows::Row;
use git2::Oid;
use gpui::{AnyElement, App, Rgba};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// What a diff the review shows is of; it reads it from the repository with its own root and settings.
pub enum Source {
    /// A commit against its first parent.
    Commit(CommitInfo),
    /// A run of commits, newest and oldest: what the newest holds against the oldest's parent — one commit against
    /// its first parent —, under a comparison's header.
    Commits { newest: Oid, oldest: Oid },
    /// Two versions of a branch compared, their commits paired.
    Versions { from: Version, to: Version },
    /// One commit of an older version against its counterpart in a newer one, of the versions `from` and `to`.
    Interdiff {
        from: usize,
        to: usize,
        old: PairCommit,
        new: PairCommit,
    },
    /// What is not committed: all of it as `git status` groups it, or what is not staged only.
    WorkingTree { only_unstaged: bool },
    /// What `head` changed against `base`, each a name and the commit it was at, since their merge base or
    /// directly; the merge request it is the diff of, so comments can go to it.
    Compare {
        base: (String, Oid),
        head: (String, Oid),
        since: bool,
        request: Option<u64>,
    },
}

/// The diff a page shows through the tab's review (`Rc<dyn Review>` in the tab's services): what it reads, let go,
/// what is shown of it.
pub trait Review {
    /// What `source` is, read and shown in place of what was; `keep` the file to open in it.
    fn open(&self, source: Source, keep: Option<String>, cx: &mut App);

    /// What `source` is, read again in place of what is shown: the file open keeps its place in the list, so a
    /// reload finds a file partly staged where it was (#22).
    fn reopen(&self, source: Source, keep: Option<String>, cx: &mut App);

    /// Nothing shown: the diff let go, its read dropped, the first file the open one.
    fn clear(&self, cx: &mut App);

    /// The diff shown, once it is read.
    fn shown(&self, cx: &App) -> Option<Arc<diff::Diff>>;

    /// How far your review of commit `tip` got, files done of all: once begun, until it is whole.
    fn head_part(&self, tip: Oid, cx: &App) -> Option<(u32, u32)>;

    /// Whether your review of commit `tip` is whole.
    fn head_reviewed(&self, tip: Oid, cx: &App) -> bool;

    /// How far your review of merge request `iid` got, against its head now (`head`).
    fn request_seen(&self, iid: u64, head: Option<Oid>, cx: &App) -> Seen;

    /// The files of the diff shown, while the card that comes up as its last one is marked is up.
    fn done(&self, cx: &App) -> Option<usize>;

    /// That card goes.
    fn close_done(&self, cx: &mut App);

    /// The diff's column with what `look` brings, its rows' pins and its clicks `host`'s.
    fn column(&self, look: ColumnLook, host: Rc<dyn Host>, cx: &mut App) -> AnyElement;
}

/// What the shell and the other features put on the diff's column (`Review::column`), drawn by them.
pub struct ColumnLook {
    /// The pane's border: whether the keyboard works in it.
    pub border: Rgba,
    /// Over the toolbar, in order: the parts' (a new request's switch, an outdated discussion's stepper).
    pub top: Vec<AnyElement>,
    /// The toolbar's left, saying which range is shown: a part's.
    pub bar: Option<AnyElement>,
    /// View ▾'s menu is open.
    pub menu_open: bool,
    /// On the file's line after its changes: the app it opens in.
    pub open_in: Option<AnyElement>,
    /// Over the rows once the last file is marked: the parts' cards.
    pub cards: Vec<AnyElement>,
    /// The file as a page, when it is shown so.
    pub alone: Option<AnyElement>,
    /// Zen shows one file at a time.
    pub zen_one: bool,
    /// The part's input with the keyboard (the find bar's field, the line typed after `c`).
    pub typing: Option<&'static str>,
}

/// The file a page reads through the tab's review.
pub trait FileView {
    /// File `path` of project `root` read at `rev` (None: as it is on disk), in place of the one read.
    fn read(&self, root: PathBuf, path: String, rev: Option<Oid>, cx: &mut App);

    /// The path of the file read, when one is.
    fn path(&self, cx: &App) -> Option<String>;

    /// The file read is let go: nothing is read.
    fn close(&self, cx: &mut App);
}

/// How far your review of a merge request got, against its head now.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Seen {
    /// No file of it was ever marked.
    Never,
    /// Files were marked at this head.
    Started,
    /// Files were marked at an earlier head: it was pushed since.
    Earlier,
    /// Its whole diff at this head was reviewed.
    Done,
}

/// What a press, a hover or a drag on a diff line asks (`Host::line`); `old` is the removed side.
pub enum LineEvent {
    /// The gutter's + was clicked: a comment on the line.
    Comment { old: bool, line: u32 },
    /// The gutter's comment mark was clicked: open or fold what is on the line.
    Toggle { old: bool, line: u32 },
    /// An annotation was clicked: the commit that wrote the line.
    Annotation { line: u32 },
    /// The mouse went down at byte `byte`, `clicks` times, with ⇧ or without.
    Press {
        old: bool,
        line: u32,
        byte: usize,
        clicks: usize,
        shift: bool,
    },
    /// The pointer moved over byte `byte` with the button down.
    Drag { old: bool, line: u32, byte: usize },
    /// A right click, at a window position.
    Context {
        old: bool,
        line: u32,
        at: gpui::Point<gpui::Pixels>,
    },
    /// The pointer came to rest on the line's number (at a window position), or went off it.
    Number {
        old: bool,
        line: u32,
        at: Option<gpui::Point<gpui::Pixels>>,
    },
    /// The hidden lines of unchanged run `segment` were asked for.
    Expand { segment: usize },
}

/// What a click on a file's header or note in the stack asks (`Host::file`).
pub enum FileEvent {
    /// File `ix` becomes the active one and folds or opens.
    Fold(usize),
    /// File `ix` (an image, which has no rows) is shown on its own.
    Alone(usize),
    /// Line `n` (from 1) of the file read in Code is given a note: its number or its + was clicked.
    Note(u32),
}

/// What a click on the Changes section asks (`Host::changes`).
pub enum ChangesEvent {
    /// File `ix`'s box: marked reviewed, or unmarked when it is.
    ToggleReviewed(usize),
    /// Folder `folder`'s box: every file under it marked (`done`) or unmarked.
    FolderReviewed { folder: String, done: bool },
    /// Mark all / Unmark all: every file the list shows.
    ReviewAll,
    /// File `ix` picked.
    Pick(usize),
    /// A right click on file `ix` at `at`: its menu.
    Menu {
        ix: usize,
        at: gpui::Point<gpui::Pixels>,
    },
    /// A folder, or a group by its key, folded or opened.
    Fold(String),
    /// The section's header: the section folds or opens.
    Section,
    /// Tree (`false`) or List (`true`).
    ListFiles(bool),
    /// A press in the section: the keyboard works in its list.
    Pressed,
}

/// What the review's host gives it to draw its rows with (the tab, for now): the rows other features pin on
/// lines, what they say beside a file's name, a document's page and its switch, whether a file's rows fade in, a
/// tree's rows fading in under a folder opened, the versions marks were made at, and what a line's, a file's, the
/// pane's and the Changes list's events do.
pub trait Host {
    /// Pinned row `row` (a discussion, a pending comment, an agent's note, the composer, the `c` prompt),
    /// `indent` from the left; `active` in the file the keyboard works in.
    fn pin(&self, row: Row, active: bool, indent: f32, cx: &App) -> gpui::AnyElement;
    /// What is said beside file `path`'s name in its header: another chat changing it (#77).
    fn badge(&self, path: &str, cx: &App) -> Option<gpui::AnyElement>;
    /// File `file`, a document, drawn as a page.
    fn page(&self, file: usize, cx: &App) -> gpui::AnyElement;
    /// File `file` just opened: its rows fade in (#56, 9).
    fn opening(&self, file: usize, cx: &App) -> bool;
    /// Event `event` on a line of file `file`; `active` false when the keyboard works in another file.
    fn line(&self, file: usize, active: bool, event: LineEvent, cx: &mut App);
    /// Event `event` on a file's header or note.
    fn file(&self, event: FileEvent, cx: &mut App);
    /// Event `event` on the file's pane.
    fn pane(&self, event: PaneEvent, cx: &mut App);
    /// Row `path` of the Changes tree, fading in when a folder just opened above it.
    fn tree_row(&self, path: &str, row: gpui::AnyElement, cx: &App) -> gpui::AnyElement;
    /// The number of the version whose tip is `tip`, of those read: the one a file was marked reviewed at.
    fn version_of(&self, tip: Oid, cx: &App) -> Option<usize>;
    /// Event `event` on the Changes list.
    fn changes(&self, event: ChangesEvent, cx: &mut App);
}

/// What a click on the file's pane asks (`Host::pane`).
#[derive(Clone)]
pub enum PaneEvent {
    /// The pane was pressed: the keyboard works in the diff.
    Pressed,
    /// ↑ or ↓ beside the file's changes: the one before or after it.
    Step { forward: bool },
    /// Generated file `path`, folded, shown anyway.
    Unfold(String),
    /// An SVG read as its source (`true`) or shown as its picture.
    SvgSource(bool),
    /// The notice's Undo: what it says changed is taken back.
    UndoNotice,
    /// The notice's ✕.
    DismissNotice,
    /// Split (`false`) or Unified (`true`).
    Unified(bool),
    /// Every file in one scroll opened (`folded`: some were) or folded.
    ExpandAll { folded: bool },
    /// View ▾ pressed: its menu hangs from `under`, the button.
    ViewMenu { under: gpui::Bounds<gpui::Pixels> },
    /// The review's chip: the open file, marked read, unmarked, else marked and the next one not read opened.
    Review,
    /// A changed image looked at another way.
    ImageMode(ImageMode),
    /// The swipe's place, or how opaque the new image is: 0 … 1.
    ImageMix(f32),
}

/// How a changed image is looked at: the two sides next to each other with their sizes (2-up), the new one wiped
/// over the old (Swipe), or laid over it (Onion skin).
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ImageMode {
    #[default]
    TwoUp,
    Swipe,
    Onion,
}

impl ImageMode {
    pub const ALL: [ImageMode; 3] = [ImageMode::TwoUp, ImageMode::Swipe, ImageMode::Onion];

    pub fn label(self) -> &'static str {
        match self {
            ImageMode::TwoUp => "2-up",
            ImageMode::Swipe => "Swipe",
            ImageMode::Onion => "Onion skin",
        }
    }
}
