//! Search in the repository (⌥⌘F, #206), the review's tool window (`artoo_review::find_files`): the tab opens it
//! and opens what it found — in the diff on screen when it changes that file, else in Code at its line; going to a
//! name and to a line in Code's file (#207).

use super::Workspace;
use super::zones::Zone;
use artoo_review_api::SEARCH;
use gpui::Context;

impl Workspace {
    /// ⌥⌘F: the repository's search opened with the keyboard, its words the find's when it has some.
    pub(super) fn search_repository(&mut self, cx: &mut Context<Self>) {
        if self.root.is_none() || self.settings_open {
            return;
        }
        self.close_palette(cx);
        self.set_tool(Some(SEARCH), cx);
    }

    /// ⇧F12 in the peek: the repository searched for `word`, the window open already or not.
    pub(super) fn search_word(&mut self, word: String, cx: &mut Context<Self>) {
        self.set_dfind(word, cx);
        if self.tool() == Some(SEARCH) {
            self.tool_opened(SEARCH, cx);
        }
        self.search_repository(cx);
    }

    /// A line the search opened: shown, the keyboard left with the search so ↓ ↵ go on to the next.
    pub(super) fn found_in_files(
        &mut self,
        found: artoo_review_api::FoundInFiles,
        cx: &mut Context<Self>,
    ) {
        self.open_found_line(found, cx);
        if self.tool() == Some(SEARCH) {
            // The find bar the line opened is looked at first: else, opened since the last look, it takes the
            // keyboard back.
            self.sync_keyboard(cx);
            self.take_keyboard(super::typing::Input::Tool, cx);
            self.set_zone(Zone::Tool, cx);
        }
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
        self.open_in_code(found.path, found.line, cx);
        self.set_dfind(found.query, cx);
    }

    /// Line `line` of `path` in Code: revealed when the file is the one read, else read there (#206, #207).
    pub(super) fn open_in_code(&mut self, path: String, line: u32, cx: &mut Context<Self>) {
        let shown = self.rv().file_read(cx).map(|v| v.path);
        if shown.as_deref() == Some(path.as_str()) && self.code_file_shown(cx) {
            self.reveal_file_line(line, cx);
        } else {
            self.file_jump = Some((path.clone(), line));
            self.open_file_view(path, cx);
        }
    }

    /// F12 in Code's file: the declarations of the name at the caret (#207).
    pub(super) fn go_to_name(&mut self, cx: &mut Context<Self>) {
        if !self.code_file_shown(cx) {
            return;
        }
        let Some(sel) = self.note_sel.clone() else {
            return;
        };
        let (line, at) = sel.head;
        if self.rv().peek_name(&sel.note, line, at, cx) {
            cx.notify();
        }
    }

    /// ⌃G in Code's file: `Line` in its head.
    pub(super) fn go_to_line(&mut self, cx: &mut Context<Self>) {
        if self.code_file_shown(cx) {
            self.rv().open_file_line(cx);
            cx.notify();
        }
    }
}
