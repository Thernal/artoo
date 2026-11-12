//! Search in the repository (⌘⌥F, #206), the review's (`artoo_review::find_files`): the tab opens it over the
//! window, gives it its field and the keys, and opens what is found — in the diff on screen when it changes that
//! file, else in Code at its line.

use super::Workspace;
use artoo_review_api::{SearchEvent, SearchKeyed as Keyed};
use gpui::{AnyElement, Context, KeyDownEvent, prelude::*};
use std::rc::Rc;

impl Workspace {
    pub(super) fn open_find_files(&mut self, cx: &mut Context<Self>) {
        if self.root.is_none() || self.settings_open {
            return;
        }
        self.close_palette(cx);
        self.field = None;
        self.reset_field_edit();
        self.rv().open_find_files(cx);
        cx.notify();
    }

    pub(super) fn close_find_files(&mut self, cx: &mut Context<Self>) {
        if self.rv().close_find_files(cx) {
            cx.notify();
        }
    }

    pub(super) fn ffind_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let rv = self.rv();
        let keyed = rv.find_files_key(&event.keystroke, &mut self.field_edit, cx);
        if let Keyed::Open(found) = keyed {
            self.open_found_line(found, cx);
        }
        cx.notify();
    }

    pub(super) fn ffind_step(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.rv().find_files_step(down, cx) {
            cx.notify();
        }
    }

    fn search_event(&mut self, event: SearchEvent, cx: &mut Context<Self>) {
        match event {
            SearchEvent::Close => return self.close_find_files(cx),
            SearchEvent::Open(ix) => {
                if let Some(found) = self.rv().open_found(ix, cx) {
                    self.open_found_line(found, cx);
                }
            }
            toggle => self.rv().find_files_toggle(toggle, cx),
        }
        cx.notify();
    }

    /// A line found: in the diff on screen when it changes that file, else read in Code (#206).
    fn open_found_line(&mut self, found: artoo_review_api::FoundInFiles, cx: &mut Context<Self>) {
        let in_diff =
            (self.shown_diff(cx)).is_some_and(|d| d.files.iter().any(|f| f.path() == found.path));
        if in_diff {
            self.set_dfind(found.query, cx);
            self.jump_to_line(&found.path, found.line, found.old, cx);
            return;
        }
        let shown = self.rv().file_read(cx).map(|v| v.path);
        if shown.as_deref() == Some(found.path.as_str()) && self.code_file_shown(cx) {
            self.reveal_file_line(found.line, cx);
        } else {
            self.file_jump = Some((found.path.clone(), found.line));
            self.open_file_view(found.path, cx);
        }
        self.set_dfind(found.query, cx);
    }

    pub(super) fn render_find_files(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let query = self.rv().find_files_query(cx)?;
        let field = self
            .field_text(&query, true, "Search the repository", cx)
            .into_any_element();
        let this = cx.weak_entity();
        let on = Rc::new(move |event, _: &mut gpui::Window, cx: &mut gpui::App| {
            this.update(cx, |this, cx| this.search_event(event, cx))
                .ok();
        });
        self.rv().render_find_files(field, on, cx)
    }
}
