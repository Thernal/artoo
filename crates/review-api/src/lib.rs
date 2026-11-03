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
use artoo_core::rows::{FileData, Pin, Row};
use artoo_core::storage::ViewOptions;
use git2::Oid;
use artoo_ui_kit::markdown::Selecting;
use gpui::{AnyElement, App, Rgba};
use std::collections::HashMap;
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

    /// The file the diff is at, by its place, and its path; the open file's rows' data.
    fn file(&self, cx: &App) -> usize;
    fn current_path(&self, cx: &App) -> Option<String>;
    fn data(&self, cx: &App) -> Option<Arc<FileData>>;
    /// The diff is narrow: buttons are their icons (`tight`); the list beside it folds (`narrow`).
    fn tight(&self, cx: &App) -> bool;
    fn narrow(&self, cx: &App) -> bool;
    /// How the diff is viewed (View ▾); the diff's list of rows; a code character's width; the repository it reads.
    fn options(&self, cx: &App) -> ViewOptions;
    fn diff_list(&self, cx: &App) -> gpui::ListState;
    fn char_width(&self, cx: &App) -> f32;
    fn root(&self, cx: &App) -> Option<PathBuf>;
    /// The marks a file's pins put in its gutter, `data` its built diff.
    fn pin_marks(&self, pins: &FilePins, data: Option<&FileData>) -> Vec<LineNote>;
    /// Two sample lines, for Settings: what a changed word looks like in `marks`.
    fn mark_preview(&self, marks: artoo_core::storage::MarkStyle) -> AnyElement;

    /// How a diff is read now (the view's options: whitespace ignored or not).
    fn settings(&self, cx: &App) -> artoo_core::git::DiffSettings;

    /// Where the diff shown is scrolled to: the file at the top and its offset, for coming back to it.
    fn place(&self, cx: &App) -> Option<(String, gpui::ListOffset)>;

    /// The next diff shown opens at `place` (a page come back to), or at its top.
    fn restore_at(&self, place: Option<(String, gpui::ListOffset)>, cx: &mut App);

    /// How far your review of commit `tip` got, files done of all: once begun, until it is whole.
    fn head_part(&self, tip: Oid, cx: &App) -> Option<(u32, u32)>;

    /// Whether your review of commit `tip` is whole.
    fn head_reviewed(&self, tip: Oid, cx: &App) -> bool;

    /// How far your review of merge request `iid` got, against its head now (`head`).
    fn request_seen(&self, iid: u64, head: Option<Oid>, cx: &App) -> Seen;

    /// How far your review of the diff shown got, files done of all, while it is one you review.
    fn progress(&self, cx: &App) -> Option<(u32, u32)>;

    /// The heads files of merge request `iid` were marked reviewed at.
    fn request_heads(&self, iid: u64, cx: &App) -> Vec<Oid>;

    /// The files of the diff shown, while the card that comes up as its last one is marked is up.
    fn done(&self, cx: &App) -> Option<usize>;

    /// That card goes.
    fn close_done(&self, cx: &mut App);

    /// The diff's column with what `look` brings, its rows' pins and its clicks `host`'s.
    fn column(&self, look: ColumnLook, host: Rc<dyn Host>, cx: &mut App) -> AnyElement;

    /// The side panel's Changes section while the diff has files, with whether it is open: `tag` the keyboard's mark
    /// on its title, what else goes on it and its clicks `host`'s. Asked from a view's own drawing.
    fn changes_section(
        &self,
        tag: String,
        host: Rc<dyn Host>,
        cx: &mut App,
    ) -> Option<(AnyElement, bool)>;
}

/// An item page's middle as the review draws it (#165 I5d): what the route on screen is of, the parts' pieces over
/// the diff, and the diff's column or a part's piece in its place. A page puts it where it goes.
#[derive(Clone)]
pub struct ItemView(pub gpui::AnyView);

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
    /// The file whose comment being written waits without the keyboard: a pencil on its row in Changes (#168).
    fn draft(&self, cx: &App) -> Option<String>;
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

// ---- what the tab and the review say to each other -----------------------------------------------------------

/// The field of the line typed after `c` among the tab's inputs.
pub const GOLINE_INPUT: &str = "review.goline";

/// The find bar's field among the tab's inputs.
pub const FIND_INPUT: &str = "review.find";

/// The path filter's field among the tab's inputs.
pub const FILTER_INPUT: &str = "review.filter";

/// The size of code at 100 %.
pub const CODE_SIZE: f32 = 12.;

/// What a key did to the line typed after `c`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GoLineKey {
    /// Esc: the marker goes.
    Close,
    /// The line changed: the marker moves once the rows are laid out again (`place_marker`).
    Moved,
    /// ↵: the comment opens there (`goline_at`).
    Finish,
    /// Nothing for the marker.
    Kept,
}

/// A hit opened: the search closes, the diff's find takes its words and the file opens at its line.
pub struct FoundInFiles {
    pub query: String,
    pub path: String,
    pub line: u32,
    pub old: bool,
}

/// A click in the search, for the tab to answer.
#[derive(Clone, Copy)]
pub enum SearchEvent {
    /// Outside its window.
    Close,
    Case,
    Word,
    Regex,
    /// Hit `ix` opened.
    Open(usize),
}

/// What a key typed in the search came to.
pub enum SearchKeyed {
    Kept,
    Closed,
    Open(FoundInFiles),
}

/// A click on a document's page or its switch, for the tab to answer.
#[derive(Clone)]
pub enum PageEvent {
    /// Document `path` read Rendered (`false`) or as Source.
    Source { path: String, to: bool },
    /// A block's count: its comments shown under it on the page, or hidden again.
    Notes { ix: usize, old: bool, first: u32 },
    /// A block's +: a comment written on the line it starts on.
    Comment { ix: usize, old: bool, first: u32 },
    /// A removed section's line, by path and first old line: shown where it was, or folded again.
    Fold(String, u32),
}

/// What answers a page's clicks.
pub type OnPage = Rc<dyn Fn(PageEvent, &mut App)>;

/// What the tab gives a document's page to draw it with.
pub struct PageLook {
    /// The comment marks on its lines.
    pub notes: Vec<LineNote>,
    /// What goes under a block (the composer, its opened comments), by side and first line.
    pub under: HashMap<(bool, u32), Vec<AnyElement>>,
    /// Split's column heads: the versions compared.
    pub heads: (String, String),
    /// A block's text can be selected and copied, as a comment's.
    pub selecting: Selecting,
    /// Just turned from Source: it fades in (#56, 15).
    pub coming: bool,
    pub on: OnPage,
}

/// A change the user did not ask for, with the way back.
pub struct Notice {
    pub text: String,
    /// The versions compared before (from, to), to go back to.
    pub undo: Option<(usize, usize)>,
}

/// A view option, for the toolbar and the View menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opt {
    Unified,
    Wrap,
    FullContext,
    Whitespace,
    OneFile,
    FoldReviewed,
}

/// How a file stands in the review.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReviewState {
    Open,
    Done,
    /// Marked, but the file is not what it was when it was marked.
    Stale,
}

/// Whose comment sits on a line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoteTone {
    /// A discussion on the merge request (GitLab).
    Request,
    /// A pending comment of one's own on the request.
    Draft,
    /// A comment kept for the agent.
    Agent,
    /// A line marked with F3.
    Bookmark,
}

/// A line that has comments: drawn as a mark in its gutter.
#[derive(Clone, Copy, Debug)]
pub struct LineNote {
    pub old: bool,
    pub line: u32,
    pub tone: NoteTone,
    pub count: usize,
}

/// A line an agent wrote on: what it is (a comment's or a note's id), the side, the line written on and its text then,
/// and the line followed to the diff shown when the text no longer matches (#46).
pub struct Anchored {
    pub id: u64,
    pub old: bool,
    pub line: u32,
    pub code: String,
    pub followed: Option<u32>,
}

/// What other features put on one file's lines.
#[derive(Default)]
pub struct FilePins {
    /// The agents' notes on the new side, in the order drawn (#151).
    pub notes: Vec<Anchored>,
    /// The agents' comments.
    pub comments: Vec<Anchored>,
    /// Rows under lines known without the text: discussions each with its replies, pending comments, the
    /// composer.
    pub fixed: Vec<Pin>,
    /// Gutter marks known without the text: bookmarks, discussions, pending comments.
    pub marks: Vec<(bool, u32, NoteTone)>,
    /// A composer is open in this file; the `c` prompt does not hang beside it.
    pub composing: bool,
}

/// What others put on the lines of each file, by path: gathered by the tab for the files a layout lays out.
pub type Pins = HashMap<String, FilePins>;

/// What other features put on the file's lines: the agents' notes, each its id, the line it was written on and
/// that line's text then, and the line of the note being written.
#[derive(Default)]
pub struct FileNotes {
    pub notes: Vec<(u64, u32, String)>,
    pub compose: Option<u32>,
}

/// What the review says to the tab that holds it, which follows (`Workspace::review_event`).
pub enum Event {
    /// A diff came, and the file to open in it; `None` when it has no files.
    Loaded(Option<usize>),
    /// A diff or a file could not be read: why.
    Failed(String),
    /// The rows of the file shown alone were built: the tab lays them out with what is pinned on its lines.
    FileBuilt,
    /// The rows of file `ix` of the stack were built: the tab lays them out with what is pinned on its lines.
    StackBuilt(usize),
    /// A file the diff does not change was read and is shown in its place (`Review::read_file`).
    FileRead,
    /// What is drawn changed (blame came, a tip to show): the tab draws again.
    Changed,
    /// The find went to row `row` of file `file` in the one scroll: the tab makes the file the active one, its
    /// pins laid out, and the row comes into view.
    Found { file: usize, row: usize },
    /// The line typed after `c` moved the marker, asks to write there, or closed: the tab lays the rows out with
    /// what is pinned on them, and opens the composer.
    GoLine(GoLineKey),
    /// A click on a document's switch (`md_page::emitting`).
    Page(PageEvent),
}

/// A row of the Changes list, as the keyboard moves over it: a file, or a folder or a group by its key.
#[derive(Clone)]
pub enum ItemKey {
    File(usize),
    Dir(String),
}
