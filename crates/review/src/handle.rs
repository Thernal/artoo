//! What the tab and its pages ask of the review (`artoo_review_api::Review`): its handle, offered in the tab's
//! services. What it changes is changed at once: ask it from anywhere but the review's own update.

use crate::changes::FileItem;
use crate::diff_view;
use crate::review::Review;
use artoo_core::rows::{FileData, Row};
use artoo_core::storage::{Settings, ViewOptions};
use artoo_review_api::diff::Diff;
use artoo_review_api::{
    Event, FileNotes, FileRead, FoundInFiles, Hears, Host, ImageMode, ItemKey, LineNote, Notice,
    OnSearch, Opt, PageLook, Pins, ReviewState, SearchEvent, SearchKeyed, Source,
};
use artoo_ui_kit::markdown::Selecting;
use git2::Oid;
use gpui::{AnyElement, App, Entity, IntoElement, WeakEntity};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// What a page asks of `review` for the diff it shows (`artoo_review_api::Review`), as the tab offers it.
pub fn handle(review: &Entity<Review>) -> Rc<dyn artoo_review_api::Review> {
    Rc::new(Handle(review.downgrade()))
}

struct Handle(WeakEntity<Review>);

impl Handle {
    /// What `f` reads of the review; nothing once it is gone.
    fn read<T: Default>(&self, cx: &App, f: impl FnOnce(&Review, &App) -> T) -> T {
        self.0
            .upgrade()
            .map(|r| f(r.read(cx), cx))
            .unwrap_or_default()
    }

    /// What `f` does to the review; nothing once it is gone.
    fn with<T: Default>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut Review, &mut gpui::Context<Review>) -> T,
    ) -> T {
        self.0.update(cx, f).unwrap_or_default()
    }
}

impl artoo_review_api::Review for Handle {
    fn changes_section(
        &self,
        tag: String,
        host: Rc<dyn Host>,
        cx: &mut App,
    ) -> Option<(gpui::AnyElement, bool)> {
        let rv = self.0.upgrade()?;
        // Asked before the review is updated: the tab's inputs read it.
        let frame = rv.read(cx).frame.as_ref().and_then(std::rc::Weak::upgrade);
        let filtering = frame.is_some_and(|f| f.typing(cx) == Some(crate::path_filter::INPUT));
        let list = crate::changes_view::ChangesLook {
            filtering,
            draft: host.draft(cx),
        };
        let look = crate::changes_view::SectionLook { tag, list };
        rv.update(cx, |rv, cx| rv.changes_section(look, host, cx))
    }

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

    fn file(&self, cx: &App) -> usize {
        self.0.upgrade().map_or(0, |r| r.read(cx).file)
    }

    fn current_path(&self, cx: &App) -> Option<String> {
        self.0.upgrade()?.read(cx).current_path()
    }

    fn data(&self, cx: &App) -> Option<Arc<FileData>> {
        self.0.upgrade()?.read(cx).data.clone()
    }

    fn tight(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|r| r.read(cx).tight)
    }

    fn narrow(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|r| r.read(cx).narrow)
    }

    fn options(&self, cx: &App) -> ViewOptions {
        self.0
            .upgrade()
            .map(|r| r.read(cx).options)
            .unwrap_or_default()
    }

    fn diff_list(&self, cx: &App) -> gpui::ListState {
        match self.0.upgrade() {
            Some(r) => r.read(cx).diff_list.clone(),
            None => gpui::ListState::new(0, gpui::ListAlignment::Top, gpui::px(0.)),
        }
    }

    fn char_width(&self, cx: &App) -> f32 {
        self.0.upgrade().map_or(0., |r| r.read(cx).char_width)
    }

    fn root(&self, cx: &App) -> Option<PathBuf> {
        self.0.upgrade()?.read(cx).root.clone()
    }

    fn pin_marks(
        &self,
        pins: &artoo_review_api::FilePins,
        data: Option<&FileData>,
    ) -> Vec<artoo_review_api::LineNote> {
        crate::pins::Laid::marks(pins, data)
    }

    fn mark_preview(&self, marks: artoo_core::storage::MarkStyle) -> gpui::AnyElement {
        diff_view::mark_preview(marks).into_any_element()
    }

    fn place(&self, cx: &App) -> Option<(String, gpui::ListOffset)> {
        self.0.upgrade()?.read(cx).scroll_place()
    }

    fn restore_at(&self, place: Option<(String, gpui::ListOffset)>, cx: &mut App) {
        if let Some(rv) = self.0.upgrade() {
            rv.update(cx, |rv, _| rv.restore_scroll = place);
        }
    }

    fn settings(&self, cx: &App) -> artoo_core::git::DiffSettings {
        artoo_core::git::DiffSettings {
            ignore_whitespace: self
                .0
                .upgrade()
                .is_some_and(|rv| rv.read(cx).options.ignore_whitespace),
        }
    }

    fn head_part(&self, tip: git2::Oid, cx: &App) -> Option<(u32, u32)> {
        self.0.upgrade()?.read(cx).head_part(tip)
    }

    fn head_reviewed(&self, tip: git2::Oid, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|rv| rv.read(cx).head_reviewed(tip))
    }

    fn progress(&self, cx: &App) -> Option<(u32, u32)> {
        let (done, total) = self.0.upgrade()?.read(cx).review_progress()?;
        Some((done as u32, total as u32))
    }

    fn request_heads(&self, iid: u64, cx: &App) -> Vec<git2::Oid> {
        let Some(rv) = self.0.upgrade() else {
            return Vec::new();
        };
        let rv = rv.read(cx);
        let Some(root) = rv.root.as_ref() else {
            return Vec::new();
        };
        let scope = format!("{}|mr:{iid}", root.display());
        rv.reviewed
            .marked_heads(&scope)
            .into_iter()
            .flatten()
            .collect()
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

    fn shown_head(&self, cx: &App) -> Option<Oid> {
        self.read(cx, |rv, _| rv.shown_head())
    }

    fn working_changed(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.blames.working_changed())
    }

    fn file_read(&self, cx: &App) -> Option<FileRead> {
        self.read(cx, |rv, _| {
            rv.file_view.as_ref().map(|v| FileRead {
                path: v.path.clone(),
                rev: v.rev(),
                at: v.at().to_owned(),
                lines: v.line_count(),
            })
        })
    }

    fn file_outline(&self, cx: &App) -> Vec<(u32, usize, &'static str, String)> {
        self.read(cx, |rv, _| rv.file_outline())
    }

    fn set_jump(&self, at: Option<(u32, bool)>, cx: &mut App) {
        self.with(cx, |rv, _| {
            rv.jump_line = at;
            rv.jump_whole = at.is_some();
        })
    }

    fn expand(&self, ix: usize, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.expanded.insert(ix))
    }

    fn set_settings(&self, settings: Settings, cx: &mut App) {
        self.with(cx, |rv, _| rv.settings = settings)
    }

    fn clear_unfolded(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.unfolded.clear())
    }

    fn find_files_open(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.find_files_open())
    }

    fn local_top(&self, cx: &App) -> usize {
        self.read(cx, |rv, _| rv.local_top())
    }

    fn rows(&self, cx: &App) -> Vec<Row> {
        self.read(cx, |rv, _| rv.rows.clone())
    }

    fn set_list_files(&self, list: bool, cx: &mut App) {
        self.with(cx, |rv, _| rv.options.list_files = list)
    }

    fn flip_dir(&self, path: &str, cx: &mut App) -> bool {
        self.with(cx, |rv, _| {
            rv.collapsed.remove(path) || {
                rv.collapsed.insert(path.to_owned());
                false
            }
        })
    }

    fn change_line(&self, cx: &App) -> Option<u32> {
        self.read(cx, |rv, _| {
            rv.changes
                .get(rv.change_at.unwrap_or(0))
                .and_then(|&row| rv.new_line_of_row(row))
        })
    }

    fn render_zen_head(&self, title: String, one: bool, cx: &App) -> AnyElement {
        match self.0.upgrade() {
            Some(rv) => rv.read(cx).render_zen_head(title, one),
            None => gpui::Empty.into_any_element(),
        }
    }

    fn line_marks(&self, cx: &App) -> Vec<LineNote> {
        self.read(cx, |rv, _| rv.notes.clone())
    }

    fn stacked(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.stack.is_some())
    }

    fn reviewed_head(&self, iid: u64, head: Option<Oid>, cx: &App) -> Option<Oid> {
        self.read(cx, |rv, _| rv.reviewed_head(iid, head))
    }

    fn changes_open(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.changes_open)
    }

    fn open_changes(&self, open: bool, keep: bool, moves: bool, cx: &mut App) {
        self.with(cx, |rv, cx| rv.open_changes(open, keep, moves, cx))
    }

    fn toggle_annotate(&self, cx: &mut App) {
        self.with(cx, |rv, _| {
            rv.annotate = !rv.annotate;
            rv.annotations.clear();
        })
    }

    fn refresh_annotations(&self, cx: &mut App) {
        self.with(cx, |rv, cx| rv.refresh_annotations(cx))
    }

    fn hover_line_number(
        &self,
        ix: usize,
        old: bool,
        line: u32,
        at: Option<gpui::Point<gpui::Pixels>>,
        cx: &mut App,
    ) -> bool {
        self.with(cx, |rv, cx| rv.hover_line_number(ix, old, line, at, cx))
    }

    fn annotation(&self, ix: usize, cx: &App) -> Option<Oid> {
        self.read(cx, |rv, _| rv.annotations.get(ix).map(|c| c.id))
    }

    fn app_settings(&self, cx: &App) -> Settings {
        self.read(cx, |rv, _| rv.settings.clone())
    }

    fn body_width(&self, cx: &App) -> f32 {
        self.read(cx, |rv, _| rv.body_width.get())
    }

    fn set_room(&self, narrow: bool, tight: bool, cx: &mut App) {
        self.with(cx, |rv, _| {
            rv.narrow = narrow;
            rv.tight = tight;
        })
    }

    fn code_size(&self, cx: &App) -> f32 {
        self.read(cx, |rv, _| rv.code_size())
    }

    fn set_char_width(&self, width: f32, cx: &mut App) {
        self.with(cx, |rv, _| rv.char_width = width)
    }

    fn set_selecting(&self, on: bool, cx: &mut App) {
        self.with(cx, |rv, _| rv.selecting = on)
    }

    fn render_blame_tip(&self, cx: &App) -> Option<AnyElement> {
        self.read(cx, |rv, _| rv.render_blame_tip())
    }

    fn clear_find(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.dfind = None)
    }

    fn line_pair(&self, line: u32, old: bool, cx: &App) -> (Option<u32>, Option<u32>) {
        self.read(cx, |rv, _| rv.line_pair(line, old))
    }

    fn diff_line(&self, line: u32, old: bool, cx: &App) -> Option<artoo_core::git::DiffLine> {
        self.read(cx, |rv, _| rv.diff_line(line, old))
    }

    fn remeasure_pages(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.remeasure_pages())
    }

    fn set_notice(&self, notice: Option<Notice>, cx: &mut App) {
        self.with(cx, |rv, _| rv.notice = notice)
    }

    fn take_notice(&self, cx: &mut App) -> Option<Notice> {
        self.with(cx, |rv, _| rv.notice.take())
    }

    fn review_scope(&self, cx: &App) -> Option<String> {
        self.read(cx, |rv, _| rv.review_scope.clone())
    }

    fn sel(&self, cx: &App) -> Option<artoo_core::select::Sel> {
        self.read(cx, |rv, _| rv.sel)
    }

    fn first_change_line(&self, cx: &App) -> Option<u32> {
        self.read(cx, |rv, _| {
            let row = *rv.changes.first()?;
            rv.new_line_of_row(row)
        })
    }

    fn set_line_notes(&self, pins: &Pins, cx: &mut App) {
        self.with(cx, |rv, _| rv.notes = rv.line_notes(pins))
    }

    fn set_jump_line(&self, at: Option<(u32, bool)>, cx: &mut App) {
        self.with(cx, |rv, _| rv.jump_line = at)
    }

    fn file_view_line(&self, line: u32, cx: &App) -> Option<String> {
        self.read(cx, |rv, _| {
            rv.file_view
                .as_ref()
                .and_then(|v| v.line(line).map(str::to_owned))
        })
    }

    fn file_lines_shown(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.file_lines_shown())
    }

    fn viewed_rev(&self, cx: &App) -> Option<Oid> {
        self.read(cx, |rv, _| rv.viewed_rev())
    }

    fn reveal_file_line(&self, line: u32, notes: &FileNotes, cx: &mut App) {
        self.with(cx, |rv, _| rv.reveal_file_line(line, notes))
    }

    fn file_top_line(&self, path: &str, notes: &FileNotes, cx: &App) -> u32 {
        self.read(cx, |rv, _| rv.file_top_line(path, notes))
    }

    fn file_reading(&self, path: &str, cx: &App) -> bool {
        self.read(cx, |rv, _| {
            rv.file_reading.is_some() && rv.file_view.as_ref().is_none_or(|v| v.path != path)
        })
    }

    fn file_binary(&self, cx: &App) -> Option<Option<u64>> {
        self.read(cx, |rv, _| rv.file_view.as_ref().map(|v| v.binary))
    }

    fn render_file_view(
        &self,
        host: Rc<dyn Host>,
        notes: &FileNotes,
        binary: Option<AnyElement>,
        selecting: Selecting,
        cx: &mut App,
    ) -> Option<(gpui::Div, AnyElement)> {
        self.with(cx, |rv, cx| {
            rv.render_file_view(host, notes, binary, selecting, cx)
        })
    }

    fn open_find_files(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.open_find_files())
    }

    fn close_find_files(&self, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.close_find_files())
    }

    fn find_files_key(
        &self,
        key: &gpui::Keystroke,
        field: &mut artoo_core::textedit::TextEdit,
        cx: &mut App,
    ) -> SearchKeyed {
        self.0
            .update(cx, |rv, cx| rv.find_files_key(key, field, cx))
            .unwrap_or(SearchKeyed::Kept)
    }

    fn find_files_step(&self, down: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.find_files_step(down))
    }

    fn open_found(&self, ix: usize, cx: &mut App) -> Option<FoundInFiles> {
        self.with(cx, |rv, _| rv.open_found(ix))
    }

    fn find_files_toggle(&self, toggle: SearchEvent, cx: &mut App) {
        self.with(cx, |rv, _| rv.find_files_toggle(toggle))
    }

    fn find_files_query(&self, cx: &App) -> Option<String> {
        self.read(cx, |rv, _| rv.find_files_query().map(str::to_owned))
    }

    fn render_find_files(&self, field: AnyElement, on: OnSearch, cx: &App) -> Option<AnyElement> {
        self.read(cx, |rv, _| rv.render_find_files(field, on))
    }

    fn press(&self, old: bool, line: u32, byte: usize, clicks: usize, shift: bool, cx: &mut App) {
        self.with(cx, |rv, _| rv.press(old, line, byte, clicks, shift))
    }

    fn drag(&self, old: bool, line: u32, byte: usize, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.drag(old, line, byte))
    }

    fn set_sel(&self, sel: Option<artoo_core::select::Sel>, cx: &mut App) {
        self.with(cx, |rv, _| rv.sel = sel)
    }

    fn clear_sel(&self, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.sel.take().is_some())
    }

    fn selected_text(&self, cx: &App) -> Option<String> {
        self.read(cx, |rv, _| rv.selected_text())
    }

    fn step_find(&self, forward: bool, cx: &mut App) -> Option<(usize, usize)> {
        self.with(cx, |rv, _| rv.step_find(forward))
    }

    fn scroll_to_row(&self, row: usize, cx: &mut App) {
        self.with(cx, |rv, _| rv.scroll_to_row(row))
    }

    fn step_change(&self, forward: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.step_change(forward))
    }

    fn done_open(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.done_card)
    }

    fn goline_open(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.goline.is_some())
    }

    fn find_open(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.dfind.is_some())
    }

    fn path_filter(&self, cx: &App) -> String {
        self.read(cx, |rv, _| rv.pfilter.clone())
    }

    fn clear_path_filter(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.pfilter.clear())
    }

    fn open_find(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.open_find())
    }

    fn close_find(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.close_find())
    }

    fn find(&self, text: String, cx: &mut App) -> Option<(usize, usize)> {
        self.with(cx, |rv, cx| rv.find(text, cx))
    }

    fn open_stack(&self, ix: usize, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.open_stack(ix))
    }

    fn reopen_stack(&self, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.reopen_stack())
    }

    fn folded(&self, ix: usize, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.folded(ix))
    }

    fn scroll_to_header(&self, ix: usize, cx: &mut App) {
        self.with(cx, |rv, _| rv.scroll_to_header(ix))
    }

    fn activate_file(&self, ix: usize, pins: &Pins, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.activate_file(ix, pins))
    }

    fn flip_fold(&self, ix: usize, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.flip_fold(ix))
    }

    fn refold(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.refold())
    }

    fn scrolled_to(&self, cx: &App) -> Option<(usize, Vec<usize>)> {
        self.read(cx, |rv, _| rv.scrolled_to())
    }

    fn review_state(&self, ix: usize, cx: &App) -> Option<ReviewState> {
        self.read(cx, |rv, _| rv.review_state(ix))
    }

    fn build_stack(&self, cx: &mut App) {
        self.with(cx, |rv, cx| rv.build_stack(cx))
    }

    fn built(&self, ix: usize, pins: &Pins, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.built(ix, pins))
    }

    fn relayout_stack(&self, pins: &Pins, cx: &mut App) {
        self.with(cx, |rv, _| rv.relayout_stack(pins))
    }

    fn any_folded(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.any_folded())
    }

    fn expand_all(&self, open: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.expand_all(open))
    }

    fn set_reviewed(&self, ix: usize, done: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.set_reviewed(ix, done))
    }

    fn set_reviewed_many(&self, ixs: &[usize], done: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.set_reviewed_many(ixs, done))
    }

    fn file_order(&self, cx: &App) -> Vec<usize> {
        self.read(cx, |rv, _| rv.file_order())
    }

    fn next_unreviewed(&self, cx: &mut App) -> Option<usize> {
        self.with(cx, |rv, _| rv.next_unreviewed())
    }

    fn scroll_file_into_view(&self, ix: usize, cx: &App) {
        self.read(cx, |rv, _| rv.scroll_file_into_view(ix))
    }

    fn fcursor(&self, cx: &App) -> Option<String> {
        self.read(cx, |rv, _| rv.fcursor.clone())
    }

    fn set_fcursor(&self, dir: Option<String>, cx: &mut App) {
        self.with(cx, |rv, _| rv.fcursor = dir)
    }

    fn generated_folded(&self, file: &artoo_core::git::FileDiff, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.generated_folded(file))
    }

    fn unfold_generated(&self, path: String, cx: &mut App) {
        self.with(cx, |rv, _| {
            rv.unfolded.insert(path);
        })
    }

    fn item_keys(&self, cx: &App) -> Vec<ItemKey> {
        self.read(cx, |rv, _| {
            rv.items()
                .into_iter()
                .map(|item| match item {
                    FileItem::File { ix, .. } => ItemKey::File(ix),
                    FileItem::Dir { path, .. } => ItemKey::Dir(path),
                    FileItem::Group { group, .. } => ItemKey::Dir(group.key().into()),
                })
                .collect()
        })
    }

    fn dir_folded(&self, dir: &str, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.collapsed.contains(dir))
    }

    fn file_key(&self, ix: usize, cx: &App) -> Option<String> {
        self.read(cx, |rv, _| rv.file_key(ix))
    }

    fn open_goline(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.open_goline())
    }

    fn selected_lines(&self, cx: &App) -> Option<(bool, u32, u32)> {
        self.read(cx, |rv, _| rv.selected_lines())
    }

    fn close_goline(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.close_goline())
    }

    fn place_marker(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.place_marker())
    }

    fn goline_at(&self, file: bool, cx: &mut App) -> Option<Result<(bool, u32), String>> {
        self.with(cx, |rv, _| rv.goline_at(file))
    }

    fn row_of_line(&self, line: u32, old: bool, cx: &App) -> Option<usize> {
        self.read(cx, |rv, _| rv.row_of_line(line, old))
    }

    fn reveal_rows(&self, rows: std::ops::Range<usize>, cx: &mut App) {
        self.with(cx, |rv, _| rv.reveal_rows(rows))
    }

    fn open_run_of(&self, line: u32, old: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.open_run_of(line, old))
    }

    fn render_goline_row(&self, indent: f32, active: bool, cx: &App) -> Option<gpui::Div> {
        self.read(cx, |rv, _| {
            rv.render_goline_row(indent, active, self.0.clone())
        })
    }

    fn take_jump(&self, cx: &mut App) -> Option<(u32, bool)> {
        self.with(cx, |rv, _| rv.jump_line.take())
    }

    fn take_jump_whole(&self, cx: &mut App) -> bool {
        self.with(cx, |rv, _| std::mem::take(&mut rv.jump_whole))
    }

    fn flip_page_fold(&self, key: (String, u32), cx: &mut App) {
        self.with(cx, |rv, _| rv.flip_page_fold(key))
    }

    fn page_rows(&self, ix: usize, cx: &App) -> Rc<Vec<artoo_core::mddiff::Row>> {
        self.read(cx, |rv, _| {
            (rv.diff.as_ref())
                .and_then(|d| d.files.get(ix))
                .map(|f| rv.page.rows(f))
                .unwrap_or_default()
        })
    }

    fn render_page(&self, ix: usize, look: PageLook, cx: &App) -> AnyElement {
        match self.0.upgrade() {
            Some(rv) => rv.read(cx).render_page(ix, look, cx),
            None => gpui::Empty.into_any_element(),
        }
    }

    fn render_page_alone(&self, ix: usize, look: PageLook, cx: &App) -> AnyElement {
        match self.0.upgrade() {
            Some(rv) => rv.read(cx).render_page_alone(ix, look, cx),
            None => gpui::Empty.into_any_element(),
        }
    }

    fn step_page(&self, forward: bool, cx: &mut App) -> Option<bool> {
        self.with(cx, |rv, _| rv.step_page(forward))
    }

    fn scroll_page(&self, down: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.scroll_page(down))
    }

    fn load_pages(&self, repo: &str, cx: &mut App) {
        self.with(cx, |rv, _| rv.page.load(repo))
    }

    fn set_page_source(&self, path: &str, source: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.page.set_source(path, source))
    }

    fn sync_stack(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.sync_stack())
    }

    fn page_is_source(&self, path: &str, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.page.is_source(path))
    }

    fn flip_page_notes(&self, path: String, old: bool, first: u32, cx: &mut App) {
        self.with(cx, |rv, _| {
            let key = (path, old, first);
            if !rv.page.notes_open.remove(&key) {
                rv.page.notes_open.insert(key);
            }
        })
    }

    fn page_notes_open(&self, cx: &App) -> Vec<(String, bool, u32)> {
        self.read(cx, |rv, _| rv.page.notes_open.iter().cloned().collect())
    }

    fn set_changes_open(&self, open: bool, cx: &mut App) {
        self.with(cx, |rv, _| rv.changes_open = open)
    }

    fn recompute_matches(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.recompute_matches())
    }

    fn set_root(&self, root: PathBuf, cx: &mut App) {
        self.with(cx, |rv, _| rv.root = Some(root))
    }

    fn restore_file_scroll(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.restore_file_scroll())
    }

    fn show_file(&self, ix: usize, file: artoo_core::git::FileDiff, cx: &mut App) {
        self.with(cx, |rv, cx| rv.show_file(ix, file, cx))
    }

    fn relayout(&self, pins: &Pins, cx: &mut App) {
        self.with(cx, |rv, _| rv.relayout(pins))
    }

    fn relayout_all(&self, pins: &Pins, cx: &mut App) {
        self.with(cx, |rv, _| rv.relayout_all(pins))
    }

    fn expand_gap(&self, segment: usize, pins: &Pins, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.expand_gap(segment, pins))
    }

    fn option(&self, opt: Opt, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.option(opt))
    }

    fn set_option(&self, opt: Opt, value: bool, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.set_option(opt, value))
    }

    fn set_mode(&self, mode: artoo_core::storage::DiffMode, cx: &mut App) -> bool {
        self.with(cx, |rv, _| rv.set_mode(mode))
    }

    fn annotating(&self, cx: &App) -> bool {
        self.read(cx, |rv, _| rv.annotate)
    }

    fn unfold(&self, path: &str, cx: &mut App) {
        self.with(cx, |rv, cx| rv.unfold(path, cx))
    }

    fn open_filter(&self, keep: bool, cx: &mut App) {
        self.with(cx, |rv, cx| rv.open_filter(keep, cx))
    }

    fn clear_file(&self, cx: &mut App) {
        self.with(cx, |rv, _| rv.clear_file())
    }

    fn page_alone_file(&self, cx: &App) -> Option<usize> {
        self.read(cx, |rv, _| {
            let ix = rv.file;
            (rv.diff.as_ref())
                .and_then(|d| d.files.get(ix))
                .filter(|f| rv.stack.is_none() && !rv.generated_folded(f) && rv.shows_page(f))
                .map(|_| ix)
        })
    }

    fn set_svg_source(&self, source: bool, cx: &mut App) {
        self.with(cx, |rv, _| rv.images.source = source)
    }

    fn set_image_mode(&self, mode: ImageMode, cx: &mut App) {
        self.with(cx, |rv, _| rv.images.mode = mode)
    }

    fn set_image_mix(&self, mix: f32, cx: &mut App) {
        self.with(cx, |rv, _| rv.images.mix = mix)
    }

    fn files_under(&self, path: &str, cx: &App) -> Vec<usize> {
        self.read(cx, |rv, _| rv.files_under(path))
    }

    fn on_event(&self, hear: Hears, cx: &mut App) {
        if let Some(review) = self.0.upgrade() {
            cx.subscribe(&review, move |_, event: &Event, cx| hear(event, cx))
                .detach();
        }
    }

    fn on_change(&self, hear: Rc<dyn Fn(&mut App)>, cx: &mut App) {
        if let Some(review) = self.0.upgrade() {
            cx.observe(&review, move |_, cx| hear(cx)).detach();
        }
    }
}
