//! A changed markdown file read as a page, the review's (`artoo_review::md_page`): the tab gives it what goes under
//! its blocks (the composer, the comments of a block opened), the marks on its lines and Split's heads, and answers
//! its clicks and its switch's (`PageEvent`).

use super::{Selection, Workspace, px};
use crate::markdown;
use crate::mddiff::Row;
use artoo_review_api::NoteTone;
use artoo_review_api::{PageEvent, PageLook};
use gpui::{AnyElement, App, Context, WeakEntity, div, prelude::*};
use std::collections::HashMap;
use std::rc::Rc;

impl Workspace {
    /// How a page's clicks come to the tab.
    fn page_on(this: WeakEntity<Self>) -> artoo_review_api::OnPage {
        Rc::new(move |event, cx| {
            this.update(cx, |this, cx| this.page_event(event, cx)).ok();
        })
    }

    /// A click on a document's page or its switch.
    pub(super) fn page_event(&mut self, event: PageEvent, cx: &mut Context<Self>) {
        match event {
            PageEvent::Source { path, to } => self.set_page_source(&path, to, cx),
            PageEvent::Notes { ix, old, first } => self.toggle_page_notes(ix, old, first, cx),
            PageEvent::Comment { ix, old, first } => self.start_page_comment(ix, old, first, cx),
            PageEvent::Fold(path, line) => {
                self.rv().flip_page_fold((path, line), cx);
                cx.notify();
            }
        }
    }

    /// What file `ix`'s page is drawn with: the marks on its lines, what goes under its blocks,
    /// Split's heads.
    fn page_look(&self, ix: usize, this: WeakEntity<Self>, cx: &App) -> PageLook {
        let diff = self.shown_diff(cx);
        let (notes, under) = match diff.as_ref().and_then(|d| d.files.get(ix)) {
            Some(file) => {
                let rows = self.rv().page_rows(ix, cx);
                let path = file.path().to_owned();
                let data = (ix == self.diff_file(cx))
                    .then(|| self.diff_data(cx))
                    .flatten();
                let pins = self.pins_of(&path, cx);
                let notes = (self.rv().pin_marks(&pins, data.as_deref()))
                    .into_iter()
                    .filter(|n| n.tone != NoteTone::Bookmark)
                    .collect();
                (notes, self.page_under(ix, &path, &rows, &this, cx))
            }
            None => (Vec::new(), HashMap::new()),
        };
        PageLook {
            notes,
            under,
            heads: self.page_heads(cx),
            selecting: self.selecting(this.clone()),
            coming: self.motion.recent(&format!("page-{ix}")),
            on: Self::page_on(this),
        }
    }

    /// File `ix` as a page in the list of every file (`Review::render_page`).
    pub(super) fn render_page(&self, ix: usize, this: WeakEntity<Self>, cx: &App) -> AnyElement {
        let look = self.page_look(ix, this, cx);
        self.rv().render_page(ix, look, cx)
    }

    /// File `ix` alone as a page that scrolls on its own (`Review::render_page_alone`).
    pub(super) fn render_page_alone(
        &self,
        ix: usize,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> AnyElement {
        let look = self.page_look(ix, this, cx);
        self.rv().render_page_alone(ix, look, cx)
    }

    /// n / p on a page viewed alone (`Review::step_page`). Whether the page took the keys.
    pub(super) fn step_page(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        match self.rv().step_page(forward, cx) {
            None => false,
            Some(went) => {
                if went {
                    cx.notify();
                }
                true
            }
        }
    }

    /// Space and ⇧space on a page viewed alone (`Review::scroll_page`). Whether the page took it.
    pub(super) fn scroll_page(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        let took = self.rv().scroll_page(down, cx);
        if took {
            cx.notify();
        }
        took
    }

    /// Document `path` read Rendered or as Source.
    pub(super) fn set_page_source(&mut self, path: &str, source: bool, cx: &mut Context<Self>) {
        if let Some(root) = &self.root {
            self.rv().load_pages(&crate::comment::key(root), cx);
        }
        if !self.rv().set_page_source(path, source, cx) {
            return;
        }
        // The other form fades in: Source's rows as a file just opened, the page as a page
        // (#56, 15).
        if let Some(ix) = self
            .shown_diff(cx)
            .as_ref()
            .and_then(|d| d.files.iter().position(|f| f.path() == path))
        {
            self.mark_motion(&format!("file-{ix}"), cx);
            self.mark_motion(&format!("page-{ix}"), cx);
        }
        self.rv().sync_stack(cx);
        self.build_stack(cx);
        if !self.rv().stacked(cx) {
            self.relayout(cx);
        }
        cx.notify();
    }

    /// M: the active file, when it is a document, turns Rendered or Source.
    pub(super) fn toggle_page_source(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .shown_diff(cx)
            .as_ref()
            .and_then(|d| d.files.get(self.diff_file(cx)))
            .map(|f| f.path().to_owned())
            .filter(|p| markdown::is_document(p))
        else {
            return;
        };
        let source = self.rv().page_is_source(&path, cx);
        self.set_page_source(&path, !source, cx);
    }

    /// The comment button of file `ix`'s third block (a snapshot's way in).
    #[cfg(feature = "snapshot")]
    pub(super) fn page_block_comment(&mut self, ix: usize, cx: &mut Context<Self>) {
        let rows = self.rv().page_rows(ix, cx);
        if let Some((_, (line, _))) = rows.get(2).and_then(|r| r.new.clone()) {
            self.start_page_comment(ix, false, line, cx);
        }
    }

    /// The count of file `ix`'s third block opened (a snapshot's way in).
    #[cfg(feature = "snapshot")]
    pub(super) fn page_block_notes(&mut self, ix: usize, cx: &mut Context<Self>) {
        let rows = self.rv().page_rows(ix, cx);
        if let Some((_, (line, _))) = rows.get(2).and_then(|r| r.new.clone()) {
            self.toggle_page_notes(ix, false, line, cx);
        }
    }

    /// A block's count: its comments shown under it on the page, or hidden again.
    fn toggle_page_notes(&mut self, ix: usize, old: bool, first: u32, cx: &mut Context<Self>) {
        let Some(path) = self
            .shown_diff(cx)
            .as_ref()
            .and_then(|d| d.files.get(ix))
            .map(|f| f.path().to_owned())
        else {
            return;
        };
        self.rv().flip_page_notes(path, old, first, cx);
        self.rv().remeasure_pages(cx);
        cx.notify();
    }

    /// What goes under each block of file `ix` (by side and first line): the composer written
    /// on it, and the comments of a block whose count was opened.
    fn page_under(
        &self,
        ix: usize,
        path: &str,
        rows: &[Row],
        this: &WeakEntity<Self>,
        cx: &gpui::App,
    ) -> HashMap<(bool, u32), Vec<AnyElement>> {
        let span = |old: bool, line: u32| {
            rows.iter()
                .filter_map(|r| if old { r.old.as_ref() } else { r.new.as_ref() })
                .map(|(_, l)| *l)
                .find(|(a, b)| (*a..=*b).contains(&line))
        };
        let mut under: HashMap<(bool, u32), Vec<AnyElement>> = HashMap::new();
        if let Some((_, old, line)) = self.page_compose().filter(|(f, _, _)| *f == ix)
            && let Some((first, _)) = span(old, line)
            && let Some(composer) = self.compose_box(this.clone(), cx)
        {
            under
                .entry((old, first))
                .or_default()
                .push(div().mt(px(8.)).child(composer).into_any_element());
        }
        for (p, old, first) in &self.rv().page_notes_open(cx) {
            let Some((first, last)) = span(*old, *first).filter(|_| p == path) else {
                continue;
            };
            let here = |o: bool, n: u32| o == *old && (first..=last).contains(&n);
            let mut notes: Vec<AnyElement> = Vec::new();
            if self.showing_request(cx) {
                notes.extend(
                    self.requests_api()
                        .map(|r| r.threads_in(path, cx))
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|&(_, o, n)| here(o, n))
                        .map(|(t, _, _)| self.render_request_thread(t, 0., this.clone(), cx)),
                );
            }
            notes.extend(
                self.comments(cx)
                    .iter()
                    .filter(|c| c.path == path && here(c.old, c.line))
                    .map(|c| self.render_thread(c.id, 0., this.clone(), cx)),
            );
            under.entry((*old, first)).or_default().extend(
                notes
                    .into_iter()
                    .map(|n| div().mt(px(8.)).child(n).into_any_element()),
            );
        }
        under
    }

    /// The column heads of Split: the versions compared.
    fn page_heads(&self, cx: &App) -> (String, String) {
        match self.selection(cx) {
            Selection::Versions { from, to } => {
                match (self.version_at(from, cx), self.version_at(to, cx)) {
                    (Some(a), Some(b)) => (
                        format!("v{} · before", a.number),
                        format!("v{} · now", b.number),
                    ),
                    _ => ("Before".into(), "Now".into()),
                }
            }
            Selection::WorkingTree => ("HEAD · before".into(), "Working tree · now".into()),
            Selection::Commit(ix) => match self.commit_info(ix, cx) {
                Some(c) => {
                    let sha = super::format::short(c.id);
                    (format!("{sha}^ · before"), format!("{sha} · now"))
                }
                None => ("Before".into(), "After".into()),
            },
            Selection::Commits { newest, oldest } => {
                match (self.commit_info(oldest, cx), self.commit_info(newest, cx)) {
                    (Some(a), Some(b)) => (
                        format!("{}^ · before", super::format::short(a.id)),
                        format!("{} · now", super::format::short(b.id)),
                    ),
                    _ => ("Before".into(), "After".into()),
                }
            }
            Selection::None => ("Before".into(), "After".into()),
        }
    }
}
