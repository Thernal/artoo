//! A file read in Code (#71, #75): read only, at Code's revision — the working tree on disk by
//! default, else a commit's —, highlighted, with its line numbers; a Markdown file rendered
//! (Source shows its lines), an image shown. A click on a line number or on the + of a hovered
//! line (or c, lines selected) writes a note for the agent on that plain line: kept on the path, the line and its text, drawn under the line while the
//! text is there (else it is outdated, in the Agents window). Opened from another space, it is
//! read as that page shows the project. A binary file says so, with Open in (#78); ⌘W closes its
//! tab, esc goes up to Code.

use super::spaces::CODE;
use super::{Header, Workspace, format};
use artoo_code_api::FilePage;
use artoo_review_api::FileNotes;
use gpui::{App, Context, prelude::*};
use std::rc::Rc;

impl Workspace {
    /// A file's lines are on screen in Code: `c` writes a note on one of them.
    pub(super) fn code_file_shown(&self, cx: &App) -> bool {
        self.nav.space == CODE && self.rv().file_lines_shown(cx)
    }

    /// Shows `path` in Code at its revision, as a tab; from another space, read as that page
    /// shows the project (#71, #75).
    pub(super) fn open_file_view(&mut self, path: String, cx: &mut Context<Self>) {
        if self.root.is_none() {
            return;
        }
        if self.nav.space != CODE && !self.plain {
            let rev = match self.shown_diff(cx).as_ref().map(|d| &d.header) {
                Some(Header::WorkingTree { .. }) => Some(None),
                Some(_) => (self.rv().viewed_rev(cx)).map(|rev| Some((format::short(rev), rev))),
                None => None,
            };
            if let Some(rev) = rev
                && let Some(code) = self.code()
            {
                code.follow(rev, cx);
            }
        }
        self.open_page(super::nav::Page::File(path), cx);
    }

    /// The file read came (`ReviewEvent::FileRead`): a note shown from the Agents window has its line come
    /// into view.
    pub(super) fn file_read(&mut self, cx: &mut Context<Self>) {
        let shown = self.rv().file_read(cx).map(|v| v.path);
        if let Some(line) = self
            .file_jump
            .take_if(|(p, _)| shown.as_ref() == Some(p))
            .map(|(_, line)| line)
        {
            self.reveal_file_line(line, cx);
        }
    }

    /// Line `line` of the file comes into view, its source shown.
    pub(super) fn reveal_file_line(&mut self, line: u32, cx: &mut Context<Self>) {
        let notes = self.file_notes(cx);
        self.rv().reveal_file_line(line, &notes, cx);
        cx.notify();
    }

    /// What is on the lines of the file read: the agents' notes and the note being written.
    fn file_notes(&self, cx: &App) -> FileNotes {
        let Some(view) = self.rv().file_read(cx) else {
            return FileNotes::default();
        };
        FileNotes {
            notes: self
                .comments(cx)
                .iter()
                .filter(|c| c.path == view.path && !c.old && !c.archived)
                .map(|c| (c.id, c.line, c.code.clone()))
                .collect(),
            compose: self
                .file_compose()
                .filter(|(p, _)| *p == view.path)
                .map(|(_, line)| line),
        }
    }

    /// The line at the top of `path`'s view as it is scrolled: where Open in opens it (#78).
    pub(super) fn file_top_line(&self, path: &str, cx: &App) -> u32 {
        let notes = self.file_notes(cx);
        self.rv().file_top_line(path, &notes, cx)
    }

    /// File `path` as Code shows it, drawn by the review: being read, read, or none.
    pub(super) fn file_page(&self, path: &str, cx: &mut Context<Self>) -> FilePage {
        // A file being read that is not on screen yet: a shimmer of lines, not the one before.
        if self.rv().file_reading(path, cx) {
            return FilePage::Reading;
        }
        let Some(binary) = self.rv().file_binary(cx) else {
            return FilePage::Empty;
        };
        let binary = binary.map(|size| self.render_binary(size, cx));
        let notes = self.file_notes(cx);
        let selecting = self.selecting(cx.entity().downgrade());
        let host: Rc<dyn artoo_review_api::Host> = Rc::new(super::TabHost(cx.entity().downgrade()));
        match (self.rv()).render_file_view(host, &notes, binary, selecting, cx) {
            Some((head, body)) => FilePage::Read(head.into_any_element(), body),
            None => FilePage::Empty,
        }
    }
}
