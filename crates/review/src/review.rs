//! The review (#165 stage 4): the diff on screen and how it is read (its files and rows, the find over it, the
//! marks of files read, the annotations), an entity the tab holds (`Workspace::review`). It loads and builds in
//! its own tasks and says what came (`ReviewEvent`); the tab follows. It draws its rows and the files' headers
//! itself, with what the tab gives it (`Host`): the rows other features pin, what a line's and a file's events do.

use crate::diff::{Diff, Header, Notice};
use crate::diff_view::{self, CODE_SIZE, DIFF_ROW, RowStyle};
use crate::pins::Pins;
use crate::select::Sel;
use artoo_core::git::{DiffSettings, FileDiff};
use artoo_core::rows::{self, FileData, Row};
use artoo_core::storage::{DiffMode, Settings, ViewOptions};
use artoo_review_api::Source;
use artoo_ui_kit::{ISLAND_RADIUS, flat, motion, px, theme};
use gpui::{
    App, Context, Entity, ListState, ScrollStrategy, ScrollWheelEvent, Task,
    UniformListScrollHandle, WeakEntity, canvas, div, list, prelude::*,
};
use std::cell::Cell as Shared;
use std::cell::Cell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

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

/// Where the diff's top is: the file, the line (and whether on the old side) of the nearest row
/// that has one, how far into that row.
pub type Anchor = (usize, Option<(u32, bool)>, gpui::Pixels);

pub struct Review {
    /// The repository's folder, a copy of the tab's (`Workspace::root`): the marks' keys, blame.
    pub root: Option<PathBuf>,
    /// The app's settings, a copy the tab keeps equal to its own (`Workspace::save_settings`): how code is
    /// drawn (its zoom, the marks of changed words), generated files folded.
    pub settings: Settings,
    /// Advance of one code character, measured on the first render and again in another face.
    pub char_width: f32,
    /// How the diff and its files are shown: split or unified, wrap, context, whitespace, the files as a list,
    /// one file at a time, reviewed files folded.
    pub options: ViewOptions,
    /// The Changes list's path filter.
    pub pfilter: String,
    /// The Changes list's keyboard cursor: the key of the file or folder it is on.
    pub fcursor: Option<String>,
    /// The comment marks of the open file's gutter (`pins.rs`).
    pub notes: Vec<diff_view::LineNote>,
    /// How changed images are shown (`image_view.rs`).
    pub images: crate::image_view::Images,
    /// How changed documents are read (`md_page.rs`).
    pub page: crate::md_page::Page,
    /// The annotation column (who wrote each line) is on, and what blame said of the open file.
    pub annotate: bool,
    pub annotations: Vec<diff_view::AnnCell>,
    pub annot_task: Option<Task<()>>,
    /// Blame for a long hover on a line number (D19), and the tip shown.
    pub blames: crate::annotate::Blames,
    pub blame_tip: Option<crate::annotate::BlameTip>,
    /// A file the diff does not change, shown in its place (D18), and its read.
    pub file_view: Option<crate::file_view::FileView>,
    pub file_reading: crate::file_view::Reading,
    /// The marks of files read, and the scope and file fingerprints of the open diff.
    pub reviewed: crate::reviewed::Store,
    /// The card that comes up when the last file of a diff is marked reviewed.
    pub done_card: bool,
    pub review_scope: Option<String>,
    pub fps: Vec<u64>,
    /// Generated files the reader asked to see in full.
    pub unfolded: HashSet<String>,
    /// The diff is narrower than 700 px: it is drawn unified whatever the option says.
    pub narrow: bool,
    /// The diff is narrower than 600 px: a comment's secondary buttons keep only their icon, the
    /// keycaps go and its indent gives way (#69).
    pub tight: bool,
    /// Where the toolbar's View ▾ was last drawn: its menu hangs from it.
    pub view_button: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The line number being typed for a quick comment.
    pub goline: Option<String>,
    /// The diff row the `c` marker stands on.
    pub goline_row: Option<usize>,
    pub jump_line: Option<(u32, bool)>,
    /// The jump shows the whole function around its line (#46 C).
    pub jump_whole: bool,
    pub diff: Option<Arc<Diff>>,
    pub file: usize,
    /// Folders closed in the file tree.
    pub collapsed: HashSet<String>,
    /// The selected text of the open diff, and whether the button is down (a drag extends it).
    pub sel: Option<Sel>,
    pub selecting: bool,
    /// The find bar's text over the open diff, and what it looks for.
    pub dfind: Option<String>,
    pub dterms: Arc<artoo_core::search::Terms>,
    /// ⌘F's `.*`: the text is a regular expression (#113); what is wrong with it, when it is.
    pub dfind_regex: bool,
    pub dfind_error: Option<String>,
    /// The find bar's field: its caret, selection and undo.
    pub find_area: artoo_ui_kit::input::Area,
    /// The path filter's field (`path_filter`), and whether it is open to take the keyboard: pressed, or ⌥⌘P.
    pub filter_area: artoo_ui_kit::input::Area,
    pub filter_open: bool,
    /// Its lists' widths: the Changes list's (`changes`).
    pub widths: artoo_ui_kit::room::Widths,
    /// The field of the line typed after `c`.
    pub goline_area: artoo_ui_kit::input::Area,
    /// Find in files (⌘⌥F), while it is open (`find_files`).
    pub ffind: Option<crate::find_files::FindFiles>,
    /// Rows where a change begins, and the one stepped to (F7).
    pub changes: Vec<usize>,
    pub change_at: Option<usize>,
    /// Rows of the open diff that hold the find text, and the one stepped to.
    pub matches: Vec<usize>,
    pub match_at: usize,
    /// Said above the diff when it changed under the user.
    pub notice: Option<Notice>,
    /// Scroll position (file path, offset) to put back once the reloaded file is laid out.
    pub restore_scroll: Option<(String, gpui::ListOffset)>,
    /// The selected file, built in the background.
    pub data: Option<Arc<FileData>>,
    /// `data` laid out for `options`.
    pub rows: Vec<Row>,
    /// Every file of the diff in one scroll (`stack.rs`); `None` while files are shown one at a
    /// time.
    pub stack: Option<crate::stack::Stack>,
    /// Unchanged runs of the selected file the user opened.
    pub expanded: HashSet<usize>,
    pub diff_list: ListState,
    /// Horizontal scroll of the diff, in pixels.
    pub offset: f32,
    /// Width of the diff body at the last paint.
    pub body_width: Rc<Shared<f32>>,
    pub file_scroll: UniformListScrollHandle,
    pub diff_task: Option<Task<()>>,
    pub view_task: Option<Task<()>>,
    /// The tab as its parts reach it, once the review's are made (`install`): a press in a field of the column
    /// asks it for the keyboard. Weak: the tab's services hold the review.
    pub frame: Option<std::rc::Weak<dyn artoo_shell_api::Frame>>,
    /// The side panel's Changes section is open (⌥⌘2), as the user left it (`changes_view::changes_left_open`) or
    /// folded for zen a while.
    pub changes_open: bool,
    /// Its toggle's mark: its rows stay drawn while it folds.
    pub changes_motion: motion::Marks,
}

/// What the review says to the tab that holds it, which follows (`Workspace::review_event`).
pub enum ReviewEvent {
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
    GoLine(crate::goline::GoLineKey),
    /// A click on a document's switch (`md_page::emitting`).
    Page(crate::md_page::PageEvent),
}

impl gpui::EventEmitter<ReviewEvent> for Review {}

/// What a line's and a file's events ask and what the review's host draws on its rows: the contract's.
pub use artoo_review_api::{ChangesEvent, FileEvent, Host, LineEvent, PaneEvent};

/// What a page asks of `review` for the diff it shows (`artoo_review_api::Review`), as the tab offers it.
pub fn handle(review: &Entity<Review>) -> Rc<dyn artoo_review_api::Review> {
    Rc::new(Handle(review.downgrade()))
}

struct Handle(WeakEntity<Review>);

impl artoo_review_api::Review for Handle {
    fn column(
        &self,
        look: artoo_review_api::ColumnLook,
        host: Rc<dyn Host>,
        cx: &mut App,
    ) -> gpui::AnyElement {
        self.0
            .update(cx, |rv, cx| rv.render_column(look, host, cx))
            .unwrap_or_else(|_| gpui::div().into_any_element())
    }

    fn open(&self, source: Source, keep: Option<String>, cx: &mut App) {
        self.0.update(cx, |rv, cx| rv.open(source, keep, cx)).ok();
    }

    fn reopen(&self, source: Source, keep: Option<String>, cx: &mut App) {
        self.0
            .update(cx, |rv, cx| {
                let at = rv.file;
                rv.clear();
                rv.file = at;
                rv.open(source, keep, cx)
            })
            .ok();
    }

    fn clear(&self, cx: &mut App) {
        self.0.update(cx, |rv, _| rv.clear()).ok();
    }

    fn shown(&self, cx: &App) -> Option<Arc<Diff>> {
        self.0.upgrade()?.read(cx).diff.clone()
    }

    fn head_part(&self, tip: git2::Oid, cx: &App) -> Option<(u32, u32)> {
        self.0.upgrade()?.read(cx).head_part(tip)
    }

    fn head_reviewed(&self, tip: git2::Oid, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|rv| rv.read(cx).head_reviewed(tip))
    }

    fn request_seen(&self, iid: u64, head: Option<git2::Oid>, cx: &App) -> artoo_review_api::Seen {
        self.0
            .upgrade()
            .map_or(artoo_review_api::Seen::Never, |rv| {
                rv.read(cx).request_seen(iid, head)
            })
    }

    fn done(&self, cx: &App) -> Option<usize> {
        let rv = self.0.upgrade()?;
        let rv = rv.read(cx);
        rv.done_card
            .then(|| rv.diff.as_ref().map(|d| d.files.len()))?
    }

    fn close_done(&self, cx: &mut App) {
        self.0
            .update(cx, |rv, cx| {
                rv.done_card = false;
                cx.notify();
            })
            .ok();
    }
}

impl Review {
    /// The rows in the diff's list (the selected file's, or every file's in one scroll), scrolled
    /// vertically by the list and horizontally by `offset`; what other features pin on lines, the
    /// stack's headers and notes, and what a line's events do come from `host`.
    pub fn render_rows(
        &self,
        host: Rc<dyn crate::review::Host>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        use crate::review::LineEvent;
        let this = cx.entity().downgrade();
        let outer = this.clone();
        let (row_host, list_host) = (host.clone(), host);
        let row = move |ix: usize, cx: &mut App| -> gpui::AnyElement {
            let host = &row_host;
            let Some(review) = this.upgrade() else {
                return div().into_any_element();
            };
            let review = review.read(cx);
            // With all files in one scroll: the file of the row, and its place among the file's
            // rows (the active file's are the review's own).
            let (file, ix) = match &review.stack {
                Some(stack) => {
                    let (b, inside) = stack.locate(ix);
                    let file = stack.blocks[b].file;
                    if inside == 0 {
                        return review.render_header(file, this.clone(), host, cx);
                    }
                    if review.body(b) != crate::stack::Body::Rows {
                        return review.render_note(b, file, this.clone(), host, cx);
                    }
                    (Some((b, file)), inside - 1)
                }
                None => (None, ix),
            };
            let active = file.is_none_or(|(_, f)| f == review.file);
            let file_ix = file.map_or(review.file, |(_, f)| f);
            let (data, row, mut style) = match file {
                Some((b, _)) if !active => {
                    let Some(stack) = &review.stack else {
                        return div().into_any_element();
                    };
                    let block = &stack.blocks[b];
                    let (Some(data), Some(&row)) = (block.data.clone(), block.rows.get(ix)) else {
                        return div().into_any_element();
                    };
                    let style = review.row_style_for(Some(&data), &block.notes, false);
                    (data, row, style)
                }
                _ => {
                    let (Some(data), Some(&row)) = (review.data.clone(), review.rows.get(ix))
                    else {
                        return div().into_any_element();
                    };
                    (data, row, review.row_style())
                }
            };
            style.strong = match (&review.stack, file) {
                (Some(stack), Some((_, f))) => stack.matches.get(stack.match_at) == Some(&(f, ix)),
                _ => review.matches.get(review.match_at) == Some(&ix),
            };
            let indent = if review.tight { 24. } else { style.gutter + 8. };
            if matches!(
                row,
                Row::Thread(_) | Row::Request(_) | Row::Draft(_) | Row::Note(_) | Row::Composer
            ) {
                return host.pin(row, active, indent, cx);
            }
            let marked = active && review.goline.is_some() && review.goline_row == Some(ix);
            let send = {
                let host = host.clone();
                move |event, cx: &mut App| host.line(file_ix, active, event, cx)
            };
            let on = || send.clone();
            let events = diff_view::Events {
                blame: {
                    let send = on();
                    Box::new(move |old, line, at, cx| send(LineEvent::Number { old, line, at }, cx))
                },
                annotate: {
                    let send = on();
                    Box::new(move |_, line, cx| send(LineEvent::Annotation { line }, cx))
                },
                toggle: {
                    let send = on();
                    Box::new(move |old, line, cx| send(LineEvent::Toggle { old, line }, cx))
                },
                comment: {
                    let send = on();
                    Box::new(move |old, line, cx| send(LineEvent::Comment { old, line }, cx))
                },
                press: {
                    let send = on();
                    Box::new(move |old, line, byte, clicks, shift, cx| {
                        send(
                            LineEvent::Press {
                                old,
                                line,
                                byte,
                                clicks,
                                shift,
                            },
                            cx,
                        )
                    })
                },
                drag: {
                    let send = on();
                    Box::new(move |old, line, byte, cx| {
                        send(LineEvent::Drag { old, line, byte }, cx)
                    })
                },
                context: {
                    let send = on();
                    Box::new(move |old, line, at, cx| {
                        send(LineEvent::Context { old, line, at }, cx)
                    })
                },
            };
            let element = diff_view::row(
                &data,
                row,
                style,
                move |segment, cx| send(LineEvent::Expand { segment }, cx),
                Rc::new(events),
            );
            if marked {
                return div()
                    .w_full()
                    .border_l_2()
                    .border_color(theme::focus())
                    .bg(theme::hover())
                    .child(element)
                    .into_any_element();
            }
            element
        };
        // With all files in one scroll, a file's rows are framed as its card.
        let rows = list(self.diff_list.clone(), move |ix, _, cx| {
            // Its own id: the ids inside a row (a line number's, its +) repeat from file to file.
            let element = div()
                .id(("list-row", ix))
                .w_full()
                .child(row(ix, cx))
                .into_any_element();
            let Some(review) = outer.upgrade() else {
                return element;
            };
            let review = review.read(cx);
            match &review.stack {
                Some(stack) if stack.locate(ix).1 > 0 => {
                    let file = stack.blocks[stack.locate(ix).0].file;
                    // A file just opened: its rows fade in (#56, 9).
                    let element = if list_host.opening(file, cx) {
                        motion::appear(
                            div().w_full().child(element),
                            ("row-opening", ix),
                            motion::BASE,
                            |row, v| row.opacity(v),
                        )
                        .into_any_element()
                    } else {
                        element
                    };
                    review.card_row(element, file, stack.last_of_block(ix))
                }
                _ => element,
            }
        })
        .flex_1();

        let width = self.body_width.clone();
        let max = self.max_offset();
        let thumb = (!self.options.wrap && max > 0.).then(|| {
            // Its track keeps clear of the island's round bottom corners.
            let inset = if flat() { 0. } else { ISLAND_RADIUS };
            let track = (self.body_width.get() - 2. * inset).max(0.);
            let share = track / (track + max);
            let thumb = (track * share).max(32.).min(track);
            div()
                .absolute()
                .bottom(px(2.))
                .h(px(4.))
                .w(px(thumb))
                .left(px(inset + (track - thumb) * self.offset / max))
                .rounded_full()
                .bg(theme::scrollbar())
        });
        div()
            .id("diff-body")
            .relative()
            // The ground between the files' cards.
            .when(self.stack.is_some(), |s| s.bg(theme::base()))
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .on_scroll_wheel(cx.listener(|rv, event, _, cx| {
                if rv.scroll_horizontally(event) {
                    cx.emit(ReviewEvent::Changed);
                }
            }))
            .font_family(theme::code_font())
            .text_size(px(self.code_size()))
            .line_height(px(self.row_height()))
            .child(rows)
            .child(
                canvas(
                    move |bounds, _, _| width.set(f32::from(bounds.size.width)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(thumb)
    }
}

impl Review {
    /// Generated file `path` shown in full anyway, its rows built.
    pub fn unfold(&mut self, path: &str, cx: &mut Context<Self>) {
        self.unfolded.insert(path.to_owned());
        self.sync_stack();
        self.build_stack(cx);
    }

    pub fn clear_file(&mut self) {
        self.stack = None;
        self.page.leave();
        self.annotations.clear();
        self.data = None;
        self.view_task = None;
        self.rows.clear();
        self.expanded.clear();
        self.sel = None;
        self.matches.clear();
        self.match_at = 0;
        self.changes.clear();
        self.change_at = None;
        self.offset = 0.;
        self.diff_list.reset(0);
    }

    /// The commit the shown diff's new side is: what a comment written now is read from.
    pub fn shown_head(&self) -> Option<git2::Oid> {
        self.diff.as_ref()?.header.head()
    }

    /// What the diff's top shows: the file, the line of the nearest row with one (none on its
    /// header), and how far into that row it is scrolled.
    pub(crate) fn scroll_anchor(&self) -> Option<Anchor> {
        let top = self.diff_list.logical_scroll_top();
        let (file, rows, inside) = match &self.stack {
            Some(stack) => {
                let (b, inside) = stack.locate(top.item_ix);
                let block = stack.blocks.get(b)?;
                let rows = if block.file == self.file {
                    &self.rows
                } else {
                    &block.rows
                };
                (block.file, rows, inside.checked_sub(1))
            }
            None => (self.file, &self.rows, Some(top.item_ix)),
        };
        let line = inside.and_then(|at| {
            rows.get(..=at.min(rows.len().checked_sub(1)?))?
                .iter()
                .rev()
                .find_map(rows::line_of)
        });
        Some((file, line, top.offset_in_item))
    }

    /// Back to what `scroll_anchor` saw, in the rows as they are laid out now.
    pub(crate) fn restore_anchor(&mut self, (file, line, offset): Anchor) {
        let item_ix = match &self.stack {
            Some(stack) => {
                let (Some(b), Some(header)) = (stack.block_of(file), stack.header(file)) else {
                    return;
                };
                let rows = if file == self.file {
                    &self.rows
                } else {
                    &stack.blocks[b].rows
                };
                match line.and_then(|l| rows::row_with(rows, l)) {
                    Some(at) if at < stack.shown(b) => header + 1 + at,
                    _ => header,
                }
            }
            None if file == self.file => match line.and_then(|l| rows::row_with(&self.rows, l)) {
                Some(at) => at,
                None => return,
            },
            None => return,
        };
        self.diff_list.scroll_to(gpui::ListOffset {
            item_ix,
            offset_in_item: offset,
        });
    }

    pub fn option(&self, opt: Opt) -> bool {
        match opt {
            Opt::Unified => self.options.unified,
            Opt::Wrap => self.options.wrap,
            Opt::FullContext => self.options.full_context,
            Opt::Whitespace => self.options.ignore_whitespace,
            Opt::OneFile => self.options.one_file,
            Opt::FoldReviewed => self.options.fold_reviewed,
        }
    }

    /// The new-side line shown at `row`, to open the editor where the reader is.
    pub fn new_line_of_row(&self, row: usize) -> Option<u32> {
        match *self.rows.get(row)? {
            rows::Row::Split { right, .. } => right.map(|c| c.line),
            rows::Row::Unified { new_line, .. } => new_line,
            _ => None,
        }
    }

    /// The revision whose files the open diff shows on its new side; none for the working tree.
    pub fn viewed_rev(&self) -> Option<git2::Oid> {
        match &self.diff.as_ref()?.header {
            Header::Commit(c) => Some(c.id),
            Header::Compare { head, .. } => Some(head.1),
            Header::Versions { to, .. } => Some(to.tip),
            Header::Interdiff { new, .. } => Some(new.id),
            Header::WorkingTree { .. } => None,
        }
    }

    /// The options the rows are laid out with: a diff under 700 px wide is unified.
    pub(crate) fn view_options(&self) -> ViewOptions {
        let mut options = self.options;
        options.unified |= self.narrow;
        options
    }

    /// The code's font size and line height with the zoom applied.
    pub fn code_size(&self) -> f32 {
        (CODE_SIZE * f32::from(self.settings.zoom) / 100.).round()
    }

    pub(crate) fn row_height(&self) -> f32 {
        (DIFF_ROW * f32::from(self.settings.zoom) / 100.).round()
    }

    /// A generated file with a long change starts folded; the reader can open it.
    pub fn generated_folded(&self, file: &FileDiff) -> bool {
        self.settings.fold_generated
            && artoo_core::generated::is_generated(file.path())
            && file.added + file.removed > artoo_core::generated::FOLD_ABOVE
            && !self.unfolded.contains(file.path())
    }

    pub(crate) fn row_style(&self) -> RowStyle<'_> {
        self.row_style_for(self.data.as_deref(), &self.notes, true)
    }

    /// How a row of a file built as `data` is drawn; the selection and blame only in the `active`
    /// file.
    pub(crate) fn row_style_for<'a>(
        &'a self,
        data: Option<&FileData>,
        notes: &'a [diff_view::LineNote],
        active: bool,
    ) -> RowStyle<'a> {
        let digits = data.map_or(3, |d| d.gutter_digits);
        RowStyle {
            wrap: self.options.wrap,
            offset: self.offset,
            marks: self.settings.mark_style,
            terms: &self.dterms,
            strong: false,
            sel: self.sel.filter(|_| active),
            notes,
            annot: self
                .annotate
                .then_some(if active { &self.annotations[..] } else { &[] }),
            row: self.row_height(),
            // Room for the comment mark (16 px) left of the number.
            gutter: digits.max(3) as f32 * self.char_width + 30.,
        }
    }

    /// How far the longest line can scroll before its end reaches the right edge.
    pub(crate) fn max_offset(&self) -> f32 {
        let Some(widest) = self.widest() else {
            return 0.;
        };
        let style = self.row_style();
        let sides = if self.view_options().unified { 1. } else { 2. };
        // A file's card is inset from the sides, and framed.
        let card = if self.stack.is_some() {
            2. * crate::stack::CARD_MARGIN + 2.
        } else {
            0.
        };
        let visible =
            (self.body_width.get() - card - style.chrome(self.view_options().unified)) / sides;
        (widest as f32 * self.char_width + 24. - visible).max(0.)
    }

    /// Lays the selected file out again for the current options, with `pins` under their lines; the diff
    /// scrolls to the top (with all files in one scroll, only its rows that changed are replaced).
    pub fn relayout(&mut self, pins: &Pins) {
        let block = self
            .stack
            .as_ref()
            .and_then(|s| s.block_of(self.file))
            .map(|b| (b, self.shown_rows(b)));
        let rows = self
            .data
            .as_ref()
            .map(|data| rows::layout(data, &self.view_options(), &self.expanded))
            .unwrap_or_default();
        self.rows = self.with_pins(rows, pins);
        self.notes = self.line_notes(pins);
        match block {
            Some((b, before)) => self.stack_changed(b, before),
            None => self.diff_list.reset(self.rows.len()),
        }
        self.offset = self.offset.min(self.max_offset());
        self.recompute_matches();
    }

    /// Every file laid out again: the open one, or all of them in one scroll. The line at the top
    /// stays there — a narrow window's unified diff and back has other rows above it (the
    /// developer, 2026-10-23: the place seemed lost).
    pub fn relayout_all(&mut self, pins: &Pins) {
        let anchor = self.scroll_anchor();
        if self.stack.is_some() {
            self.relayout_stack(pins);
        } else {
            self.relayout(pins);
        }
        if let Some(anchor) = anchor {
            self.restore_anchor(anchor);
        }
    }

    /// Shows the hidden lines of an unchanged run in place, keeping the scroll position; false when
    /// there is nothing to show.
    pub fn expand_gap(&mut self, segment: usize, pins: &Pins) -> bool {
        let Some(data) = self.data.clone() else {
            return false;
        };
        if self.stack.is_some() {
            self.expanded.insert(segment);
            self.relayout(pins);
            return true;
        }
        let Some(at) = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::Gap { segment: s, .. } if *s == segment))
        else {
            return false;
        };
        self.expanded.insert(segment);
        let rows = self.with_pins(
            rows::layout(&data, &self.view_options(), &self.expanded),
            pins,
        );
        let inserted = rows.len() + 1 - self.rows.len();
        self.rows = rows;
        self.diff_list.splice(at..at + 1, inserted);
        self.recompute_matches();
        true
    }

    /// No diff any more.
    pub fn clear(&mut self) {
        self.unfolded.clear();
        self.notice = None;
        self.diff = None;
        self.diff_task = None;
        self.clear_file();
        self.file = 0;
    }

    /// Shows what `source` is, read with the review's root and settings in its task (`load`); `keep` is the file
    /// to open in it. Nothing without a root.
    pub fn open(&mut self, source: Source, keep: Option<String>, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let settings = DiffSettings {
            ignore_whitespace: self.options.ignore_whitespace,
        };
        self.load(keep, move || crate::diff::read(&root, source, settings), cx);
    }

    /// Loads a diff with `load` in the background; once it came, it is shown (`Review::loaded`) and said
    /// (`ReviewEvent::Loaded`), or why it could not be read.
    pub fn load(
        &mut self,
        keep: Option<String>,
        load: impl FnOnce() -> anyhow::Result<Diff> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.diff_task = Some(cx.spawn(async move |this, cx| {
            let loaded = cx.background_executor().spawn(async move { load() }).await;
            this.update(cx, |rv, cx| {
                cx.emit(match loaded {
                    Ok(diff) => ReviewEvent::Loaded(rv.loaded(diff, keep)),
                    Err(err) => ReviewEvent::Failed(format!("{err:#}")),
                });
            })
            .ok();
        }));
    }

    /// Diff `diff` came: it replaces the one shown and is indexed. The file to open in it is returned (`keep`
    /// while it is there), `None` when it has no files.
    pub(crate) fn loaded(&mut self, diff: Diff, keep: Option<String>) -> Option<usize> {
        // The file asked for, else the one a page comes back to (#71): the one at the same place
        // while it is still there — a file partly staged is listed twice (#22) —, else the first
        // with its path.
        let keep = keep.or_else(|| self.restore_scroll.as_ref().map(|(path, _)| path.clone()));
        let ix = keep.and_then(|path| {
            let at = self.file;
            (diff.files.get(at).map(|f| f.path()) == Some(path.as_str()))
                .then_some(at)
                .or_else(|| diff.files.iter().position(|f| f.path() == path))
        });
        let empty = diff.files.is_empty();
        self.diff = Some(Arc::new(diff));
        // The files of the diff it replaces are gone.
        self.stack = None;
        self.index();
        // A request opens at its first file not reviewed (#72); anything else at the first file as
        // the island lists it (a tree sorts folders first), so → walks on from the top.
        let request = matches!(
            self.diff.as_ref().map(|d| &d.header),
            Some(Header::Compare {
                request: Some(_),
                ..
            })
        );
        let order = self.file_order();
        let unread = order
            .iter()
            .copied()
            .find(|&ix| self.review_state(ix) != Some(crate::reviewed::State::Done));
        let ix = ix
            .or(unread.filter(|_| request))
            .or_else(|| order.first().copied())
            .unwrap_or(0);
        self.file_scroll.scroll_to_item(0, ScrollStrategy::Top);
        (!empty).then_some(ix)
    }

    /// File `ix` (`file`) alone becomes the one shown; its rows are built in the background, then said
    /// (`ReviewEvent::FileBuilt`).
    pub fn show_file(&mut self, ix: usize, file: FileDiff, cx: &mut Context<Self>) {
        self.file = ix;
        self.fcursor = None;
        self.clear_file();
        self.scroll_file_into_view(ix);
        let mode = self.options.mode;
        self.view_task = Some(cx.spawn(async move |this, cx| {
            let data = cx
                .background_executor()
                .spawn(async move { FileData::build(&file, mode) })
                .await;
            this.update(cx, |rv, cx| {
                rv.data = Some(Arc::new(data));
                cx.emit(ReviewEvent::FileBuilt);
            })
            .ok();
        }));
    }

    /// The place a reload kept, when it was in the file shown alone.
    pub fn restore_file_scroll(&mut self) {
        if let Some((path, offset)) = self.restore_scroll.take()
            && self
                .diff
                .as_ref()
                .and_then(|d| d.files.get(self.file))
                .map(|f| f.path())
                == Some(path.as_str())
        {
            self.diff_list.scroll_to(offset);
        }
    }

    /// Turns view option `opt` on or off; false when it already was.
    pub fn set_option(&mut self, opt: Opt, value: bool) -> bool {
        if self.option(opt) == value {
            return false;
        }
        match opt {
            Opt::Unified => self.options.unified = value,
            Opt::Wrap => self.options.wrap = value,
            Opt::FullContext => self.options.full_context = value,
            Opt::Whitespace => self.options.ignore_whitespace = value,
            Opt::OneFile => self.options.one_file = value,
            Opt::FoldReviewed => self.options.fold_reviewed = value,
        }
        true
    }

    /// What changed lines mark inside becomes `mode`; false when it already was.
    pub fn set_mode(&mut self, mode: DiffMode) -> bool {
        if self.options.mode == mode {
            return false;
        }
        self.options.mode = mode;
        true
    }

    /// A sideways scroll of the rows, when they do not wrap; false when nothing moved.
    pub fn scroll_horizontally(&mut self, event: &ScrollWheelEvent) -> bool {
        if self.options.wrap {
            return false;
        }
        let delta = f32::from(event.delta.pixel_delta(px(self.row_height())).x);
        if delta == 0. {
            return false;
        }
        let offset = (self.offset - delta).clamp(0., self.max_offset());
        if offset == self.offset {
            return false;
        }
        self.offset = offset;
        true
    }
}
