//! A file read in Code: what it was read at, its lines highlighted with their tabs widened, a Markdown file
//! rendered until Source, an image decoded, a binary file's size; and where a note on a line sits now.

use artoo_core::highlight::Span;
use git2::Oid;
use gpui::{ListState, SharedString, Task};
use std::rc::Rc;
use std::sync::Arc;

pub struct FileView {
    pub path: String,
    /// What it was read at: a short id, or the working tree.
    pub at: String,
    /// The commit it was read at; None the working tree.
    pub rev: Option<Oid>,
    pub lines: Vec<String>,
    /// Its lines with their tabs widened, and their highlights.
    pub text: Rc<Vec<(SharedString, Vec<Span>)>>,
    /// Those lines as the selection copies them (#93).
    pub plain: Rc<std::cell::RefCell<Vec<String>>>,
    pub list: ListState,
    /// A Markdown file: shown rendered, until Source.
    pub rendered: bool,
    /// An image file, decoded.
    pub image: Option<Arc<gpui::Image>>,
    /// A binary file that is no image: its size, and nothing to show (#78).
    pub binary: Option<u64>,
}

/// The file being read for the view.
pub type Reading = Option<Task<()>>;

/// A row of the file's list: a line, a note under it, or the note being written.
#[derive(Clone, Copy)]
pub enum Row {
    Line(usize),
    Note(u64),
    Composer,
}

impl FileView {
    /// What it was read at: a short id, or the working tree.
    pub fn at(&self) -> &str {
        &self.at
    }

    /// The commit it was read at; None the working tree.
    pub fn rev(&self) -> Option<Oid> {
        self.rev
    }

    /// Line `line` (1-based) as it is.
    pub fn line(&self, line: u32) -> Option<&str> {
        self.lines
            .get((line as usize).checked_sub(1)?)
            .map(String::as_str)
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn markdown(&self) -> bool {
        self.path.ends_with(".md") || self.path.ends_with(".markdown")
    }
}

/// Where a note on `code` written at line `line` sits now among `lines`: its line while the text
/// is there, else the nearest line with that text; None when it is gone (outdated).
pub fn place_note(lines: &[String], line: u32, code: &str) -> Option<usize> {
    let at = (line as usize).checked_sub(1)?;
    if lines.get(at).is_some_and(|l| l == code) {
        return Some(at);
    }
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| *l == code)
        .min_by_key(|(n, _)| n.abs_diff(at))
        .map(|(n, _)| n)
}

/// A line with its tabs as four spaces, and its spans moved to match; an empty line is a space.
pub fn widen(line: &str, spans: &[Span]) -> (SharedString, Vec<Span>) {
    let mut out = String::with_capacity(line.len());
    // Where each byte of `line` (and its end) lands in `out`.
    let mut at = Vec::with_capacity(line.len() + 1);
    for (i, c) in line.char_indices() {
        at.resize(i + 1, out.len());
        if c == '\t' {
            out.push_str("    ");
        } else {
            out.push(c);
        }
    }
    at.resize(line.len() + 1, out.len());
    let spans = spans
        .iter()
        .filter(|s| s.range.end <= line.len())
        .map(|s| Span {
            range: at[s.range.start]..at[s.range.end],
            ..s.clone()
        })
        .collect();
    if out.is_empty() {
        out.push(' ');
    }
    (out.into(), spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabs_widen_and_the_colours_follow() {
        let span = |r: std::ops::Range<usize>| Span {
            range: r,
            color: 1,
            italic: false,
            bold: false,
        };
        let (text, spans) = widen("\tfn x", &[span(1..3)]);
        assert_eq!(text.as_ref(), "    fn x");
        assert_eq!(spans[0].range, 4..6);
        assert_eq!(widen("", &[]).0.as_ref(), " ");
    }

    #[test]
    fn a_note_follows_its_text_and_is_gone_with_it() {
        let lines: Vec<String> = ["a", "fun round()", "b", "fun round()"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // Still where it was written.
        assert_eq!(place_note(&lines, 2, "fun round()"), Some(1));
        // Lines came above it: the nearest line with its text.
        assert_eq!(place_note(&lines, 3, "fun round()"), Some(1));
        assert_eq!(place_note(&lines, 4, "fun round()"), Some(3));
        // Its text is gone: outdated.
        assert_eq!(place_note(&lines, 2, "fun round(mode)"), None);
    }
}
