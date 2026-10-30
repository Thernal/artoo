//! The review's state (#165 stage 4): the diff on screen and how it is read (its files and rows, the find over
//! it, the marks of files read, the annotations), an entity the tab holds (`Workspace::review`) on its way to
//! `artoo-review`. It loads and builds in its own tasks and says what came (`ReviewEvent`); the tab follows. It
//! draws its rows and the files' headers itself, with what the tab gives it (`Host`): the rows other features pin,
//! what a line's and a file's events do.

use super::*;

pub(super) struct Review {
    /// The repository's folder, a copy of the tab's (`Workspace::root`): the marks' keys, blame.
    pub(super) root: Option<PathBuf>,
    /// The app's settings, a copy the tab keeps equal to its own (`Workspace::save_settings`): how code is
    /// drawn (its zoom, the marks of changed words), generated files folded.
    pub(super) settings: Settings,
    /// Advance of one code character, measured on the first render and again in another face.
    pub(super) char_width: f32,
    /// How the diff and its files are shown: split or unified, wrap, context, whitespace, the files as a list,
    /// one file at a time, reviewed files folded.
    pub(super) options: ViewOptions,
    /// The Changes list's path filter.
    pub(super) pfilter: String,
    /// The Changes list's keyboard cursor: the key of the file or folder it is on.
    pub(super) fcursor: Option<String>,
    /// The comment marks of the open file's gutter (`pins.rs`).
    pub(super) notes: Vec<diff_view::LineNote>,
    /// How changed images are shown (`image_view.rs`).
    pub(super) images: image_view::Images,
    /// How changed documents are read (`md_page.rs`).
    pub(super) page: md_page::Page,
    /// The annotation column (who wrote each line) is on, and what blame said of the open file.
    pub(super) annotate: bool,
    pub(super) annotations: Vec<diff_view::AnnCell>,
    pub(super) annot_task: Option<Task<()>>,
    /// Blame for a long hover on a line number (D19), and the tip shown.
    pub(super) blames: annotate::Blames,
    pub(super) blame_tip: Option<annotate::BlameTip>,
    /// A file the diff does not change, shown in its place (D18), and its read.
    pub(super) file_view: Option<file_view::FileView>,
    pub(super) file_reading: file_view::Reading,
    /// The marks of files read, and the scope and file fingerprints of the open diff.
    pub(super) reviewed: reviewed::Store,
    /// The card that comes up when the last file of a diff is marked reviewed.
    pub(super) done_card: bool,
    pub(super) review_scope: Option<String>,
    pub(super) fps: Vec<u64>,
    /// Generated files the reader asked to see in full.
    pub(super) unfolded: HashSet<String>,
    /// The diff is narrower than 700 px: it is drawn unified whatever the option says.
    pub(super) narrow: bool,
    /// The diff is narrower than 600 px: a comment's secondary buttons keep only their icon, the
    /// keycaps go and its indent gives way (#69).
    pub(super) tight: bool,
    /// The line number being typed for a quick comment.
    pub(super) goline: Option<String>,
    /// The diff row the `c` marker stands on.
    pub(super) goline_row: Option<usize>,
    pub(super) jump_line: Option<(u32, bool)>,
    /// The jump shows the whole function around its line (#46 C).
    pub(super) jump_whole: bool,
    pub(super) diff: Option<Arc<Diff>>,
    pub(super) file: usize,
    /// Folders closed in the file tree.
    pub(super) collapsed: HashSet<String>,
    /// The selected text of the open diff, and whether the button is down (a drag extends it).
    pub(super) sel: Option<Sel>,
    pub(super) selecting: bool,
    /// The find bar's text over the open diff, and what it looks for.
    pub(super) dfind: Option<String>,
    pub(super) dterms: Arc<crate::search::Terms>,
    /// ⌘F's `.*`: the text is a regular expression (#113); what is wrong with it, when it is.
    pub(super) dfind_regex: bool,
    pub(super) dfind_error: Option<String>,
    /// Rows where a change begins, and the one stepped to (F7).
    pub(super) changes: Vec<usize>,
    pub(super) change_at: Option<usize>,
    /// Rows of the open diff that hold the find text, and the one stepped to.
    pub(super) matches: Vec<usize>,
    pub(super) match_at: usize,
    /// Said above the diff when it changed under the user.
    pub(super) notice: Option<Notice>,
    /// Scroll position (file path, offset) to put back once the reloaded file is laid out.
    pub(super) restore_scroll: Option<(String, gpui::ListOffset)>,
    /// The selected file, built in the background.
    pub(super) data: Option<Arc<FileData>>,
    /// `data` laid out for `options`.
    pub(super) rows: Vec<Row>,
    /// Every file of the diff in one scroll (`stack.rs`); `None` while files are shown one at a
    /// time.
    pub(super) stack: Option<stack::Stack>,
    /// Unchanged runs of the selected file the user opened.
    pub(super) expanded: HashSet<usize>,
    pub(super) diff_list: ListState,
    /// Horizontal scroll of the diff, in pixels.
    pub(super) offset: f32,
    /// Width of the diff body at the last paint.
    pub(super) body_width: Rc<Shared<f32>>,
    pub(super) file_scroll: UniformListScrollHandle,
    pub(super) diff_task: Option<Task<()>>,
    pub(super) view_task: Option<Task<()>>,
}

/// What the review says to the tab that holds it, which follows (`Workspace::review_event`).
pub(super) enum ReviewEvent {
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
}

impl gpui::EventEmitter<ReviewEvent> for Review {}

/// What a press, a hover or a drag on a diff line asks (`Host::line`); `old` is the removed side.
pub(super) enum LineEvent {
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
pub(super) enum FileEvent {
    /// File `ix` becomes the active one and folds or opens.
    Fold(usize),
    /// File `ix` (an image, which has no rows) is shown on its own.
    Alone(usize),
}

/// What the tab gives the review to draw its rows with (`TabHost`): the rows other features pin on lines, what
/// they say beside a file's name, a document's page and its switch, whether a file's rows fade in, and what a
/// line's and a file's events do.
pub(super) trait Host {
    /// Pinned row `row` (a discussion, a pending comment, an agent's note, the composer, the `c` prompt),
    /// `indent` from the left; `active` in the file the keyboard works in.
    fn pin(&self, row: Row, active: bool, indent: f32, cx: &App) -> gpui::AnyElement;
    /// What is said beside file `path`'s name in its header: another chat changing it (#77).
    fn badge(&self, path: &str, cx: &App) -> Option<gpui::AnyElement>;
    /// The Rendered · Source switch of file `file`, a document.
    fn page_switch(&self, file: usize, cx: &App) -> Option<gpui::AnyElement>;
    /// File `file`, a document, drawn as a page.
    fn page(&self, file: usize, cx: &App) -> gpui::AnyElement;
    /// File `file` just opened: its rows fade in (#56, 9).
    fn opening(&self, file: usize, cx: &App) -> bool;
    /// Event `event` on a line of file `file`; `active` false when the keyboard works in another file.
    fn line(&self, file: usize, active: bool, event: LineEvent, cx: &mut App);
    /// Event `event` on a file's header or note.
    fn file(&self, event: FileEvent, cx: &mut App);
}
