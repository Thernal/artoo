//! Who wrote each line (⌥⌘B, and a long hover on a line number, D19): the revision a side is read at, the blame read
//! so far, the tip under the pointer.

use artoo_core::git::BlameLine;
use artoo_ui_kit::format;
use artoo_ui_kit::hover::TipOwned;
use gpui::{AnyView, SharedString, WeakEntity};
use std::rc::Rc;

/// The revision a side of the diff is read at, for its blame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlameAt {
    Rev(git2::Oid),
    /// A commit's first parent: the old side of a commit's diff.
    ParentOf(git2::Oid),
    Head,
    /// The files on disk.
    Working,
}

/// A side of a file: where it is read, and its path there.
pub type Side = (BlameAt, String);

/// Blame read for the line numbers' long hover: by side and path, and the tips waiting for it
/// (each with its line).
#[derive(Default)]
pub struct Blames {
    pub read: std::collections::HashMap<Side, Rc<Vec<Option<BlameLine>>>>,
    pub waiting: std::collections::HashMap<Side, Vec<(WeakEntity<TipOwned>, u32)>>,
}

impl Blames {
    /// Forgets what the working tree's files said: they changed.
    pub fn working_changed(&mut self) {
        self.read.retain(|(at, _), _| *at != BlameAt::Working);
    }
}

/// The line number under the pointer, and the tip saying who wrote it once the pointer rested.
pub struct BlameTip {
    pub key: (usize, bool, u32),
    pub at: gpui::Point<gpui::Pixels>,
    pub view: Option<AnyView>,
    pub _wait: gpui::Task<()>,
}

/// What line `line` (1-based) of a blame says.
pub fn blame_text(lines: &[Option<BlameLine>], line: u32) -> SharedString {
    match line.checked_sub(1).and_then(|i| lines.get(i as usize)) {
        Some(Some(b)) => format!(
            "{}\n{} · {} · {}",
            b.summary,
            format::short(b.id),
            b.author,
            artoo_ui_kit::hover::full_date(b.time)
        )
        .into(),
        Some(None) => "Not committed yet".into(),
        // The read failed, or the file is shorter there.
        None => "Who wrote it is not known here".into(),
    }
}
