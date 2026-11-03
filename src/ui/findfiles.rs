//! Find in files (⌘⌥F), the review's (`artoo_review::find_files`): the tab opens it over the
//! window, gives it its field and the keys, and opens what is found.

use super::Workspace;
use artoo_review_api::{SearchEvent, SearchKeyed as Keyed};
use gpui::{AnyElement, Context, KeyDownEvent, prelude::*};
use std::rc::Rc;

impl Workspace {
    pub(super) fn open_find_files(&mut self, cx: &mut Context<Self>) {
        if self.shown_diff(cx).is_none() || self.settings_open {
            return;
        }
        self.palette = None;
        self.field = None;
        self.reset_field_edit();
        self.review.update(cx, |rv, _| rv.open_find_files());
        cx.notify();
    }

    pub(super) fn close_find_files(&mut self, cx: &mut Context<Self>) {
        if self.review.update(cx, |rv, _| rv.close_find_files()) {
            cx.notify();
        }
    }

    pub(super) fn ffind_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let field = &mut self.field_edit;
        let keyed = self
            .review
            .update(cx, |rv, cx| rv.find_files_key(&event.keystroke, field, cx));
        if let Keyed::Open(found) = keyed {
            self.set_dfind(found.query, cx);
            self.jump_to_line(&found.path, found.line, found.old, cx);
        }
        cx.notify();
    }

    pub(super) fn ffind_step(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.review.update(cx, |rv, _| rv.find_files_step(down)) {
            cx.notify();
        }
    }

    fn search_event(&mut self, event: SearchEvent, cx: &mut Context<Self>) {
        match event {
            SearchEvent::Close => return self.close_find_files(cx),
            SearchEvent::Open(ix) => {
                if let Some(found) = self.review.update(cx, |rv, _| rv.open_found(ix)) {
                    self.set_dfind(found.query, cx);
                    self.jump_to_line(&found.path, found.line, found.old, cx);
                }
            }
            toggle => self.review.update(cx, |rv, _| rv.find_files_toggle(toggle)),
        }
        cx.notify();
    }

    pub(super) fn render_find_files(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let query = self.review.read(cx).find_files_query()?.to_owned();
        let field = self
            .field_text(&query, true, "Search the diff", cx)
            .into_any_element();
        let this = cx.weak_entity();
        let on = Rc::new(move |event, _: &mut gpui::Window, cx: &mut gpui::App| {
            this.update(cx, |this, cx| this.search_event(event, cx))
                .ok();
        });
        self.review.read(cx).render_find_files(field, on)
    }
}
