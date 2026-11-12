//! The review's contract (#165 stage 4): what a page asks of the tab's review. What a diff is of (`Source`): a
//! commit, a run of commits, two versions, an interdiff, the working tree, a comparison — the review reads it, a page
//! opening one through the review's handle (`Review`), which draws the diff's column with what the shell and the
//! other features put on it (`ColumnLook`). The file Code reads (#71, #75): read at a revision, which one is read, closed. The review draws it; the page puts it
//! where it goes. What the review's host draws on its rows and does with their events (`Host`, `LineEvent`,
//! `FileEvent`, `PaneEvent`, `ChangesEvent`). And how far your review of a merge request got (`Seen`), which the requests list groups by.
//!
//! The tab's review offers it as `Rc<dyn FileView>` in the tab's services (`Frame::services`). What it changes is
//! changed at once: ask it from anywhere but the review's own update.

/// The diff's least width, in the design's pixels, for Split to be drawn split; narrower it is unified and Split
/// says why (#186). The diff's width decides it, never the window's.
pub const SPLIT_WIDTH: f32 = 700.;

/// What of the diff's toolbar went into View ▾ for want of room (#187), each with those before it: Collapse all, then
/// Split · Unified, then the count beside the range goes. The range itself never folds.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ToolbarFold {
    #[default]
    None,
    Collapse,
    Layout,
    Count,
}

/// What a request's Changes says when it shows what changed since the version you read (#187): that version's
/// number — each file it lists carries `↻ v2` —, and how many of the request's files are as they were then.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SinceRead {
    pub version: usize,
    pub unchanged: Option<usize>,
}

pub mod decorations;
pub mod diff;
pub mod viewers;

use artoo_core::git::{CommitInfo, PairCommit, Version};
use artoo_core::rows::{FileData, Pin, Row};
use artoo_core::storage::{Settings, ViewOptions};
use artoo_ui_kit::markdown::Selecting;
use git2::Oid;
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
    /// What is not committed: all of it as `git status` groups it, or what is not staged only; with `since`, the
    /// checked-out branch's whole work against its main line, what is not committed with it (#209).
    WorkingTree {
        only_unstaged: bool,
        since: Option<diff::Since>,
    },
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

    /// The new line at the top of the diff's view, or the first under it that has one: the line a stop of Back
    /// names (#194).
    fn top_line(&self, cx: &App) -> Option<u32>;

    /// The next diff shown opens at `place` (a page come back to), or at its top.
    fn restore_at(&self, place: Option<(String, gpui::ListOffset)>, cx: &mut App);

    /// The diff shown scrolls back to `place` (⌘[ to where a jump left, #194); false when its file is not the one
    /// shown alone, nor in the one scroll.
    fn go_place(&self, place: (String, gpui::ListOffset), cx: &mut App) -> bool;

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

    /// The diff's head: the commit its new side is at.
    fn shown_head(&self, cx: &App) -> Option<Oid>;

    /// What of the diff's toolbar is in View ▾ for want of room, as the toolbar was last measured (#187).
    fn toolbar_fold(&self, cx: &App) -> ToolbarFold;

    /// The diff shown is what changed since the version you read of a request, as the tab says once it is in
    /// (#187): its files carry `↻ v2`, its Changes say what they leave out. None for any other diff.
    fn set_since(&self, since: Option<SinceRead>, cx: &mut App);

    /// The working tree changed: what blame read of it goes.
    fn working_changed(&self, cx: &mut App);

    /// The file Code reads: its path, the revision it is at in words, its lines.
    fn file_read(&self, cx: &App) -> Option<FileRead>;

    /// The declarations of the file Code reads: (1-based line, depth, kind, name).
    fn file_outline(&self, cx: &App) -> Vec<(u32, usize, &'static str, String)>;

    /// The line to go to once the file is built (`None`: none), its whole run of unchanged lines opened.
    fn set_jump(&self, at: Option<(u32, bool)>, cx: &mut App);

    /// Row `ix`'s run of unchanged lines opened: true when it was not.
    fn expand(&self, ix: usize, cx: &mut App) -> bool;

    /// The app's settings, a copy the tab keeps equal to its own.
    fn set_settings(&self, settings: Settings, cx: &mut App);

    /// Generated files unfolded go back to folded.
    fn clear_unfolded(&self, cx: &mut App);

    /// Find in files is open.
    fn find_files_open(&self, cx: &App) -> bool;

    /// The active file's row at the top of the view.
    fn local_top(&self, cx: &App) -> usize;

    /// The rows of the file shown alone.
    fn rows(&self, cx: &App) -> Vec<Row>;

    /// The Changes list as a flat list (`true`) or a tree.
    fn set_list_files(&self, list: bool, cx: &mut App);

    /// Folder `path` of the Changes list opened if it was folded (true), else folded.
    fn flip_dir(&self, path: &str, cx: &mut App) -> bool;

    /// The new line of the change the keyboard is at.
    fn change_line(&self, cx: &App) -> Option<u32>;

    /// Zen's head: `title` says what is shown; `one` file at a time or all.
    fn render_zen_head(&self, title: String, one: bool, cx: &App) -> AnyElement;

    /// The marks of the lines of the file shown that have comments.
    fn line_marks(&self, cx: &App) -> Vec<LineNote>;

    /// All files in one scroll (the stack).
    fn stacked(&self, cx: &App) -> bool;

    /// The head of request `iid` your review got to, when it is not `head`.
    fn reviewed_head(&self, iid: u64, head: Option<Oid>, cx: &App) -> Option<Oid>;

    /// The Changes list is open.
    fn changes_open(&self, cx: &App) -> bool;

    /// The Changes list opened or closed; `keep` the view where it is, `moves` animated.
    fn open_changes(&self, open: bool, keep: bool, moves: bool, cx: &mut App);

    /// Blame beside the lines turned on or off.
    fn toggle_annotate(&self, cx: &mut App);

    /// Blame read again for the file shown.
    fn refresh_annotations(&self, cx: &mut App);

    /// The pointer came on line number `line` of file `ix` (`at`), or went off it: true when the tip shown went.
    fn hover_line_number(
        &self,
        ix: usize,
        old: bool,
        line: u32,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut App,
    ) -> bool;

    /// The commit blame says line `ix` (0-based) of the file shown is from.
    fn annotation(&self, ix: usize, cx: &App) -> Option<Oid>;

    /// The app's settings as the review has them.
    fn app_settings(&self, cx: &App) -> Settings;

    /// The diff's body's width, as last drawn.
    fn body_width(&self, cx: &App) -> f32;

    /// The diff narrow (its secondary buttons their icons) and tight (under 600 px of the design).
    fn set_room(&self, narrow: bool, tight: bool, cx: &mut App);

    /// The size code is drawn at, the settings' zoom applied.
    fn code_size(&self, cx: &App) -> f32;

    /// Advance of one code character, measured in the face drawn.
    fn set_char_width(&self, width: f32, cx: &mut App);

    /// A drag selecting text in the diff started or ended.
    fn set_selecting(&self, on: bool, cx: &mut App);

    /// Blame's tip over a line number, when one shows.
    fn render_blame_tip(&self, cx: &App) -> Option<AnyElement>;

    /// The diff's find closed, its words gone.
    fn clear_find(&self, cx: &mut App);

    /// Line `line` of a side's other side's line numbers: (old, new).
    fn line_pair(&self, line: u32, old: bool, cx: &App) -> (Option<u32>, Option<u32>);

    /// Line `line` of a side of the file shown, as the host speaks of it.
    fn diff_line(&self, line: u32, old: bool, cx: &App) -> Option<artoo_core::git::DiffLine>;

    /// Documents' pages measured again: what goes under their blocks changed.
    fn remeasure_pages(&self, cx: &mut App);

    /// A change the reviewer did not ask for, with the way back (`None`: it goes).
    fn set_notice(&self, notice: Option<Notice>, cx: &mut App);

    /// The notice shown, taken away.
    fn take_notice(&self, cx: &mut App) -> Option<Notice>;

    /// The key of the review's marks: the repository and what the diff is of.
    fn review_scope(&self, cx: &App) -> Option<String>;

    /// The text selected in the diff.
    fn sel(&self, cx: &App) -> Option<artoo_core::select::Sel>;

    /// The new line of the file shown's first change.
    fn first_change_line(&self, cx: &App) -> Option<u32>;

    /// The marks of the file shown's lines, from what is pinned on them.
    fn set_line_notes(&self, pins: &Pins, cx: &mut App);

    /// The line to go to once the file is built (`None`: none).
    fn set_jump_line(&self, at: Option<(u32, bool)>, cx: &mut App);

    /// Line `line` (1-based) of the file Code reads.
    fn file_view_line(&self, line: u32, cx: &App) -> Option<String>;

    /// The file Code reads shows its lines (not a document rendered, an image or nothing).
    fn file_lines_shown(&self, cx: &App) -> bool;

    /// The revision the file Code reads is at (`None`: the working tree).
    fn viewed_rev(&self, cx: &App) -> Option<Oid>;

    /// How many of each commit `ids`' files you viewed, of how many (#209); none for a commit whose files are not
    /// read yet — asking starts reading them, and the review says `Event::Changed` when they came.
    fn commit_viewed(&self, ids: &[Oid], cx: &mut App) -> Vec<Option<(usize, usize)>>;

    /// Line `line` of the file Code reads in view, `notes` placed among its lines.
    fn reveal_file_line(&self, line: u32, notes: &FileNotes, cx: &mut App);

    /// The line at the top of file `path` read in Code, `notes` placed among its lines.
    fn file_top_line(&self, path: &str, notes: &FileNotes, cx: &App) -> u32;

    /// A file is being read that is not on screen yet (`path` not the one shown).
    fn file_reading(&self, path: &str, cx: &App) -> bool;

    /// The file Code reads is binary: its size (`Some(None)`: text; `None`: no file).
    fn file_binary(&self, cx: &App) -> Option<Option<u64>>;

    /// The file Code reads drawn: its head and its body; `binary` what stands for a binary file's lines. `typing`:
    /// the tab's input that has the keyboard — its find bar's (#206), its `Line` (#207).
    #[allow(clippy::too_many_arguments)]
    fn render_file_view(
        &self,
        host: Rc<dyn Host>,
        notes: &FileNotes,
        binary: Option<AnyElement>,
        selecting: Selecting,
        typing: Option<&'static str>,
        cx: &mut App,
    ) -> Option<(gpui::Div, AnyElement)>;

    /// Find in files opened.
    fn open_find_files(&self, cx: &mut App);

    /// Find in files closed: true when it was open.
    fn close_find_files(&self, cx: &mut App) -> bool;

    /// A key typed in find in files, edited in the tab's `field`.
    fn find_files_key(
        &self,
        key: &gpui::Keystroke,
        field: &mut artoo_core::textedit::TextEdit,
        cx: &mut App,
    ) -> SearchKeyed;

    /// Find in files' selection moved down or up: true when it moved.
    fn find_files_step(&self, down: bool, cx: &mut App) -> bool;

    /// Hit `ix` of find in files opened: it closes, and what to open.
    fn open_found(&self, ix: usize, cx: &mut App) -> Option<FoundInFiles>;

    /// A toggle of find in files (case, word, regular expression).
    fn find_files_toggle(&self, toggle: SearchEvent, cx: &mut App);

    /// What find in files looks for, while it is open.
    fn find_files_query(&self, cx: &App) -> Option<String>;

    /// Find in files drawn, `field` the tab's field of its words.
    fn render_find_files(&self, field: AnyElement, on: OnSearch, cx: &App) -> Option<AnyElement>;

    /// A press on a line at byte `byte`: the caret, a word or the line, by click count; ⇧ extends.
    fn press(&self, old: bool, line: u32, byte: usize, clicks: usize, shift: bool, cx: &mut App);

    /// The pointer moved over a line with the button down: the selection reaches it. True when it moved.
    fn drag(&self, old: bool, line: u32, byte: usize, cx: &mut App) -> bool;

    /// The text selected in the diff (`None`: none).
    fn set_sel(&self, sel: Option<artoo_core::select::Sel>, cx: &mut App);

    /// The selection in the diff gone: true when there was one.
    fn clear_sel(&self, cx: &mut App) -> bool;

    /// The selection's text.
    fn selected_text(&self, cx: &App) -> Option<String>;

    /// The diff's find steps to the next match (or the one before): its file and row.
    fn step_find(&self, forward: bool, cx: &mut App) -> Option<(usize, usize)>;

    /// Row `row` of the file shown comes into view.
    fn scroll_to_row(&self, row: usize, cx: &mut App);

    /// The keyboard steps to the next change (or the one before): true when it moved; false past the file's last (or
    /// before its first), where the tab goes on to the next file (#191).
    fn step_change(&self, forward: bool, cx: &mut App) -> bool;

    /// The card saying every file is reviewed shows.
    fn done_open(&self, cx: &App) -> bool;

    /// The line typed after `c` is open.
    fn goline_open(&self, cx: &App) -> bool;

    /// The diff's find is open.
    fn find_open(&self, cx: &App) -> bool;

    /// The Changes list's path filter.
    fn path_filter(&self, cx: &App) -> String;

    /// The Changes list's path filter emptied.
    fn clear_path_filter(&self, cx: &mut App);

    /// The diff's find opened.
    fn open_find(&self, cx: &mut App);

    /// The diff's find closed.
    fn close_find(&self, cx: &mut App);

    /// The diff's find looks for `text`: the match it went to, its file and row.
    fn find(&self, text: String, cx: &mut App) -> Option<(usize, usize)>;

    /// All files in one scroll, file `ix` the active one: true when the stack is to be built.
    fn open_stack(&self, ix: usize, cx: &mut App) -> bool;

    /// The stack opened again where it was: true when it is to be built.
    fn reopen_stack(&self, cx: &mut App) -> bool;

    /// File `ix` of the stack is folded.
    fn folded(&self, ix: usize, cx: &App) -> bool;

    /// File `ix`'s header at the top of the stack.
    fn scroll_to_header(&self, ix: usize, cx: &mut App);

    /// File `ix` of the stack the active one, laid out with `pins`: true when it changed.
    fn activate_file(&self, ix: usize, pins: &Pins, cx: &mut App) -> bool;

    /// File `ix` of the stack folded or unfolded: true when the stack is to be built.
    fn flip_fold(&self, ix: usize, cx: &mut App) -> bool;

    /// Reviewed files folded again, as the option says.
    fn refold(&self, cx: &mut App);

    /// The file the stack scrolled to, and those scrolled past.
    fn scrolled_to(&self, cx: &App) -> Option<(usize, Vec<usize>)>;

    /// How file `ix` stands in the review.
    fn review_state(&self, ix: usize, cx: &App) -> Option<ReviewState>;

    /// The stack's files built.
    fn build_stack(&self, cx: &mut App);

    /// File `ix` of the stack was built: laid out with `pins`; true when it is the active one.
    fn built(&self, ix: usize, pins: &Pins, cx: &mut App) -> bool;

    /// The stack's files laid out again with `pins`.
    fn relayout_stack(&self, pins: &Pins, cx: &mut App);

    /// A file of the stack is folded.
    fn any_folded(&self, cx: &App) -> bool;

    /// Every file of the stack opened (or folded): true when it is to be built.
    fn expand_all(&self, open: bool, cx: &mut App) -> bool;

    /// File `ix` marked reviewed, or not: true when it changed.
    fn set_reviewed(&self, ix: usize, done: bool, cx: &mut App) -> bool;

    /// Files `ixs` marked reviewed, or not: true when one changed.
    fn set_reviewed_many(&self, ixs: &[usize], done: bool, cx: &mut App) -> bool;

    /// The files in the order the Changes list shows them.
    fn file_order(&self, cx: &App) -> Vec<usize>;

    /// The next file not reviewed, after the one shown.
    fn next_unreviewed(&self, cx: &mut App) -> Option<usize>;

    /// File `ix`'s row of the Changes list in view.
    fn scroll_file_into_view(&self, ix: usize, cx: &App);

    /// The folder of the Changes list the keyboard is on.
    fn fcursor(&self, cx: &App) -> Option<String>;

    /// The keyboard on a folder of the Changes list (`None`: on a file).
    fn set_fcursor(&self, dir: Option<String>, cx: &mut App);

    /// File `file` is generated and folded.
    fn generated_folded(&self, file: &artoo_core::git::FileDiff, cx: &App) -> bool;

    /// Generated file `path` unfolded.
    fn unfold_generated(&self, path: String, cx: &mut App);

    /// What the Changes list shows, as keys for keyboard movement.
    fn item_keys(&self, cx: &App) -> Vec<ItemKey>;

    /// Folder `dir` of the Changes list is folded.
    fn dir_folded(&self, dir: &str, cx: &App) -> bool;

    /// File `ix`'s key in the Changes list: its group and path.
    fn file_key(&self, ix: usize, cx: &App) -> Option<String>;

    /// The line after `c` opened: it starts empty, standing for the first change.
    fn open_goline(&self, cx: &mut App);

    /// F12 on Code's file (#207): the declarations of the name at byte `at` of line `line` (0-based), the caret
    /// there in the text that selects as `note`, open under the line. Whether there was a name there.
    fn peek_name(&self, note: &str, line: usize, at: usize, cx: &mut App) -> bool;

    /// ⌃G on Code's file: `Line` in its head.
    fn open_file_line(&self, cx: &mut App);

    /// The lines selected in the diff: side, first and last.
    fn selected_lines(&self, cx: &App) -> Option<(bool, u32, u32)>;

    /// The line after `c` closed.
    fn close_goline(&self, cx: &mut App);

    /// The marker on the typed line's row, in view.
    fn place_marker(&self, cx: &mut App);

    /// The lines the marker is on, first and last on their side (`file`: of the file Code reads), or why there are
    /// none; it closes.
    fn goline_at(&self, file: bool, cx: &mut App) -> Option<Result<(bool, u32, u32), String>>;

    /// The row of line `line` of a side in the file shown.
    fn row_of_line(&self, line: u32, old: bool, cx: &App) -> Option<usize>;

    /// Rows `rows` of the file shown in view.
    fn reveal_rows(&self, rows: std::ops::Range<usize>, cx: &mut App);

    /// The folded run of unchanged lines holding `line` opened: true when there was one.
    fn open_run_of(&self, line: u32, old: bool, cx: &mut App) -> bool;

    /// The prompt of `c` under the marked line; `active` it has the keyboard.
    fn render_goline_row(&self, indent: f32, active: bool, cx: &App) -> Option<gpui::Div>;

    /// The line to go to once the file is built, taken.
    fn take_jump(&self, cx: &mut App) -> Option<(u32, bool)>;

    /// Whether the line gone to opens its whole run of unchanged lines, taken.
    fn take_jump_whole(&self, cx: &mut App) -> bool;

    /// A removed section's line of a document, by path and first old line: shown where it was, or folded again.
    fn flip_page_fold(&self, key: (String, u32), cx: &mut App);

    /// File `ix`'s blocks as its page shows them.
    fn page_rows(&self, ix: usize, cx: &App) -> Rc<Vec<artoo_core::mddiff::Row>>;

    /// File `ix` as a page in the list of every file.
    fn render_page(&self, ix: usize, look: PageLook, cx: &App) -> AnyElement;

    /// File `ix` alone as a page that scrolls on its own.
    fn render_page_alone(&self, ix: usize, look: PageLook, cx: &App) -> AnyElement;

    /// n / p on a page viewed alone: `None` when it is not one, else whether it went.
    fn step_page(&self, forward: bool, cx: &mut App) -> Option<bool>;

    /// Space and ⇧space on a page viewed alone: whether it took them.
    fn scroll_page(&self, down: bool, cx: &mut App) -> bool;

    /// The documents read as Source in repository `repo`, read from disk.
    fn load_pages(&self, repo: &str, cx: &mut App);

    /// Document `path` read as Source (`true`) or Rendered: true when it changed.
    fn set_page_source(&self, path: &str, source: bool, cx: &mut App) -> bool;

    /// The stack's files follow how documents are read.
    fn sync_stack(&self, cx: &mut App);

    /// Document `path` is read as Source.
    fn page_is_source(&self, path: &str, cx: &App) -> bool;

    /// A block's comments shown under it on its page, or hidden again.
    fn flip_page_notes(&self, path: String, old: bool, first: u32, cx: &mut App);

    /// The blocks whose comments are shown under them: path, side, first line.
    fn page_notes_open(&self, cx: &App) -> Vec<(String, bool, u32)>;

    /// The Changes list open or closed, at once.
    fn set_changes_open(&self, open: bool, cx: &mut App);

    /// The diff's find's matches found again.
    fn recompute_matches(&self, cx: &mut App);

    /// The repository's folder, a copy of the tab's: the marks' keys, blame.
    fn set_root(&self, root: PathBuf, cx: &mut App);

    /// The file shown scrolls back where it was before it was built again.
    fn restore_file_scroll(&self, cx: &mut App);

    /// File `ix` (`file`) alone becomes the one shown; its rows are built in the background, then said.
    fn show_file(&self, ix: usize, file: artoo_core::git::FileDiff, cx: &mut App);

    /// The file shown laid out again with `pins`.
    fn relayout(&self, pins: &Pins, cx: &mut App);

    /// Every file laid out again with `pins`.
    fn relayout_all(&self, pins: &Pins, cx: &mut App);

    /// The hidden lines of unchanged run `segment` shown in place, laid out with `pins`: true when they were hidden.
    fn expand_gap(&self, segment: usize, pins: &Pins, cx: &mut App) -> bool;

    /// View option `opt` is on.
    fn option(&self, opt: Opt, cx: &App) -> bool;

    /// View option `opt` turned on or off: true when it changed.
    fn set_option(&self, opt: Opt, value: bool, cx: &mut App) -> bool;

    /// How the diff is drawn (split, unified, …): true when it changed.
    fn set_mode(&self, mode: artoo_core::storage::DiffMode, cx: &mut App) -> bool;

    /// Blame is shown beside the lines.
    fn annotating(&self, cx: &App) -> bool;

    /// Generated file `path` shown in full anyway, its rows built.
    fn unfold(&self, path: &str, cx: &mut App);

    /// The Changes list's path filter open to take the keyboard; `keep`: as the user leaves it.
    fn open_filter(&self, keep: bool, cx: &mut App);

    /// No file shown: the next one is built anew.
    fn clear_file(&self, cx: &mut App);

    /// The file shown, when it is shown alone as a page that scrolls on its own.
    fn page_alone_file(&self, cx: &App) -> Option<usize>;

    /// How images are looked at.
    fn set_image_mode(&self, mode: ImageMode, cx: &mut App);

    /// Where the swipe is, or how opaque the new image is: 0 … 1.
    fn set_image_mix(&self, mix: f32, cx: &mut App);

    /// The files under folder `path` of the Changes list.
    fn files_under(&self, path: &str, cx: &App) -> Vec<usize>;

    /// What the review says (`Event`), heard by `hear` as long as it lives.
    fn on_event(&self, hear: Hears, cx: &mut App);

    /// Any change of the review, heard by `hear` as long as it lives.
    fn on_change(&self, hear: Rc<dyn Fn(&mut App)>, cx: &mut App);
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
    /// Open in, on the header of the card the keyboard works in (#188): the file in its app, ▾ the others.
    fn open_in(&self, cx: &App) -> Option<gpui::AnyElement>;
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

/// The declarations of a name, open under its line in Code's file, among the tab's inputs (#207).
pub const PEEK_INPUT: &str = "review.peek";

/// ⌃G's `Line` in Code's file head among the tab's inputs (#207).
pub const LINE_INPUT: &str = "review.file-line";

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
    /// `n` past the file's last change, or `p` before its first (#191): the marker goes on in the next (or the
    /// previous) file.
    Carry { forward: bool },
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
    /// A line marked with B (a bookmark).
    Bookmark,
    /// A decorator's mark, in its colour (`0xRRGGBB`, `decorations::Mark`).
    Marked(u32),
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
    /// A declaration chosen in Code's peek (#207): the tab opens `path` in Code at 1-based `line`.
    Go { path: String, line: u32 },
    /// ⇧F12 in the peek: the tab searches the repository for the word.
    SearchWord(String),
    /// A click on a document's switch (`md_page::emitting`).
    Page(PageEvent),
}

/// A row of the Changes list, as the keyboard moves over it: a file, or a folder or a group by its key.
#[derive(Clone)]
pub enum ItemKey {
    File(usize),
    Dir(String),
}

/// The file Code reads: its path, the revision it is at in words, how many lines it has.
pub struct FileRead {
    pub path: String,
    /// The revision it is at (`None`: the working tree).
    pub rev: Option<Oid>,
    pub at: String,
    pub lines: usize,
}

/// What answers find in files' clicks.
pub type OnSearch = Rc<dyn Fn(SearchEvent, &mut gpui::Window, &mut App)>;

/// What hears the review's events (`Review::on_event`).
pub type Hears = Rc<dyn Fn(&Event, &mut App)>;
