//! The review's state (#165 stage 4): the diff on screen and how it is read (its files and rows, the find over
//! it, the marks of files read, the annotations), gathered from the tab's fields on its way to an entity of its
//! own and then to `artoo-review`.

use super::*;

pub(super) struct Review {
    /// How the diff and its files are shown: split or unified, wrap, context, whitespace, the files as a list,
    /// one file at a time, reviewed files folded.
    pub(super) options: ViewOptions,
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
    pub(super) diff: Option<Diff>,
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
