//! The composer (#9, #51, #129, #168): a comment being written — under a line of the diff, on a page, a file or a
//! folder in Code, for the agent, a merge request, a reply or an edit — and its own editing: completion, markdown
//! helpers, preview. Where it is drawn, its keys routed and what it writes sent are the tab's.

use crate::complete::Choice;
use artoo_core::markdown::{self, Helper};
use artoo_core::textedit::TextEdit;
use artoo_ui_kit::input::{self, Edit};
use gpui::{App, Keystroke};

/// Who a comment is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    /// Kept on this Mac and copied for a coding agent.
    Agent,
    /// Posted to a merge request on GitLab, where everyone on it sees it.
    Request(u64),
    /// A reply to discussion number `0` of the open merge request.
    Reply(usize),
    /// A new text for one's own comment.
    Edit(Change),
    /// A purple note on discussion `0` of the request, kept on this Mac for the agent (#50 G).
    ThreadNote(usize),
    /// The reviewer's note answering the agent's question on comment `0` (#10).
    Answer(u64),
    /// Text's own value on the agent's form of comment `0` (#10 C).
    AnswerValue(u64),
    /// Written in the Inbox: a reply to the selected notice's discussion, or a comment on its
    /// request when it has none (`Workspace::inbox_view.reply`).
    InboxReply,
}

/// The comment an edit changes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// A comment for the agent, by its id.
    Agent(u64),
    /// Note `note` of discussion number `thread` of the open merge request.
    Note { thread: usize, note: u64 },
}

/// A comment being written.
pub struct Compose {
    pub path: String,
    pub old: bool,
    /// The first line of a comment on several lines; it sits under its last, `line`.
    pub first: Option<u32>,
    pub line: u32,
    pub code: String,
    /// The text, its caret and selection.
    pub edit: TextEdit,
    pub at: String,
    pub target: Target,
    /// Why the last attempt to post failed.
    pub error: Option<String>,
    /// The text is shown formatted instead of as written (⌘⇧P); typing goes on and the preview
    /// follows it.
    pub preview: bool,
    /// The completion list's selected row, and where a list was closed with esc (#51, #9).
    pub pick: usize,
    pub closed: Option<usize>,
    /// Written on a document read as a page: its file in the diff, the composer under the block
    /// (#11).
    pub page: Option<usize>,
    /// Written on a file read in Code, at its commit (None: the working tree) — a note on a plain
    /// line, under it in the file (#75).
    pub file: Option<Option<git2::Oid>>,
    /// The agent's note it answers, carried into the comment (#151).
    pub answers: Option<String>,
}

impl Compose {
    /// The discussion of the open request a reply, an edit or a note for the agent is in.
    pub fn thread(&self) -> Option<usize> {
        match self.target {
            Target::Reply(ix) | Target::ThreadNote(ix) => Some(ix),
            Target::Edit(Change::Note { thread, .. }) => Some(thread),
            _ => None,
        }
    }

    /// That discussion moved to `ix` in the list.
    pub fn set_thread(&mut self, ix: usize) {
        match &mut self.target {
            Target::Reply(i) | Target::ThreadNote(i) => *i = ix,
            Target::Edit(Change::Note { thread, .. }) => *thread = ix,
            _ => {}
        }
    }

    /// ↑ ↓ ↵ ⇥ esc while the list of choices for the token at `at` is open: None when it did not take the key,
    /// else whether the text changed.
    pub fn completion_key(
        &mut self,
        key: &Keystroke,
        at: usize,
        choices: &[Choice],
    ) -> Option<bool> {
        if key.modifiers.number_of_modifiers() > 0 {
            return None;
        }
        match key.key.as_str() {
            "down" => self.pick = (self.pick + 1).min(choices.len() - 1),
            "up" => self.pick = self.pick.saturating_sub(1),
            "enter" | "tab" => {
                let choice = &choices[self.pick.min(choices.len() - 1)];
                let caret = self.edit.caret();
                let end = at + choice.insert.len();
                self.edit.replace(at..caret, &choice.insert, end..end);
                self.pick = 0;
                return Some(true);
            }
            "escape" => self.closed = Some(at),
            _ => return None,
        }
        Some(false)
    }

    /// ⌘B ⌘I ⇧⌘X ⇧⌘E ⇧⌘K and ⇧⌘P (Ctrl on Windows and Linux), no other modifier.
    pub fn shortcut(key: &Keystroke) -> Option<Short> {
        let m = key.modifiers;
        if !m.secondary() || m.number_of_modifiers() != 1 + u8::from(m.shift) {
            return None;
        }
        match (key.key.as_str(), m.shift) {
            ("b", false) => Some(Short::Helper(Helper::Bold)),
            ("i", false) => Some(Short::Helper(Helper::Italic)),
            ("x", true) => Some(Short::Helper(Helper::Strike)),
            ("e", true) => Some(Short::Helper(Helper::Code)),
            ("k", true) => Some(Short::Helper(Helper::Link)),
            ("p", true) => Some(Short::Preview),
            _ => None,
        }
    }

    /// A markdown helper of the toolbar, on the selection or at the caret.
    pub fn apply(&mut self, helper: Helper) {
        markdown::apply(&mut self.edit, helper);
    }

    /// The text shown formatted, or as written.
    pub fn toggle_preview(&mut self) {
        self.preview = !self.preview;
    }

    /// `text` put at the caret (a reply that mentions the author).
    pub fn insert(&mut self, text: &str) {
        self.edit.insert(text);
    }

    /// A click in the text: the caret goes there; ⇧ extends the selection, a double click takes the word; a drag
    /// (`drag`) selects.
    pub fn place(&mut self, at: usize, shift: bool, clicks: usize, drag: bool, lower: bool) {
        self.edit.click(at, clicks, shift || drag);
        if clicks == 1 && !shift && !drag {
            self.edit.set_downstream(lower);
        }
    }

    /// Key `key` on the text: ↵ a new line that keeps a list going, ⇧↵ a plain one.
    pub fn edit_key(&mut self, key: &Keystroke, cx: &mut App) -> Typed {
        match input::edit_text(&mut self.edit, key, cx) {
            // Written in, it keeps what was written and lets the keyboard go (#168).
            Edit::Escape if !self.edit.text().trim().is_empty() => Typed::Leave,
            Edit::Escape => Typed::Cancel,
            Edit::Ignored => Typed::Ignored,
            Edit::Enter { shift: true } => {
                self.edit.insert("\n");
                Typed::Changed
            }
            Edit::Enter { shift: false } => {
                markdown::newline(&mut self.edit);
                Typed::Changed
            }
            Edit::Changed | Edit::Selected => Typed::Changed,
        }
    }
}

/// A key of the composer's own beside its text (`Compose::shortcut`).
pub enum Short {
    Helper(Helper),
    /// ⇧⌘P: the text formatted or as written.
    Preview,
}

/// What a key did to the text (`Compose::edit_key`).
pub enum Typed {
    /// Esc on text written: it is kept, the keyboard let go.
    Leave,
    /// Esc on nothing written: the comment goes.
    Cancel,
    /// Not a key of the text.
    Ignored,
    /// The text, its caret or selection changed: the box may grow.
    Changed,
}
