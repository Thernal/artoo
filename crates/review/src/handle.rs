//! What the tab and its pages ask of the review (`artoo_review_api::Review`): its handle, offered in the tab's
//! services. What it changes is changed at once: ask it from anywhere but the review's own update.

use crate::diff_view;
use crate::review::Review;
use artoo_core::rows::{FileData, Row};
use artoo_core::storage::{Settings, ViewOptions};
use artoo_review_api::diff::Diff;
use artoo_review_api::{FileRead, Host, LineNote, Source};
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
        self.0.upgrade().map(|r| f(r.read(cx), cx)).unwrap_or_default()
    }

    /// What `f` does to the review; nothing once it is gone.
    fn with<T: Default>(&self, cx: &mut App, f: impl FnOnce(&mut Review, &mut gpui::Context<Review>) -> T) -> T {
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

    fn hover_line_number(&self, ix: usize, old: bool, line: u32, at: Option<gpui::Point<gpui::Pixels>>, cx: &mut App) -> bool {
        self.with(cx, |rv, cx| rv.hover_line_number(ix, old, line, at, cx))
    }

    fn annotation(&self, ix: usize, cx: &App) -> Option<Oid> {
        self.read(cx, |rv, _| rv.annotations.get(ix).map(|c| c.id))
    }
}
