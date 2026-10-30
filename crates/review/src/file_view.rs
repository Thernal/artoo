//! A file read in Code: what it was read at, its lines highlighted with their tabs widened, a Markdown file
//! rendered until Source, an image decoded, a binary file's size; and where a note on a line sits now.

use crate::review::{Review, ReviewEvent};
use artoo_core::git::{ImageKind, Repo};
use artoo_core::highlight::{self, Span};
use artoo_ui_kit::{format, px};
use git2::Oid;
use gpui::{Context, ListAlignment, ListState, SharedString, Task};
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

impl Review {
    /// Reads `path` at `rev` (the working tree when `None`) in the background; once read it is the file
    /// shown (`ReviewEvent::FileRead`), or why it could not be.
    pub fn read_file(
        &mut self,
        root: std::path::PathBuf,
        path: String,
        rev: Option<Oid>,
        cx: &mut Context<Self>,
    ) {
        let file = path.clone();
        self.file_reading = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let at = match rev {
                        Some(rev) => format::short(rev),
                        None => "the working tree".to_owned(),
                    };
                    if let Some(kind) = ImageKind::of(&file) {
                        let bytes = match rev {
                            Some(rev) => Repo::open(&root).ok()?.bytes_at(rev, &file)?,
                            None => std::fs::read(root.join(&file)).ok()?,
                        };
                        let image = gpui::Image::from_bytes(crate::image_view::format(kind), bytes);
                        return Some((Vec::new(), Vec::new(), at, Some(Arc::new(image)), None));
                    }
                    let bytes = match rev {
                        Some(rev) => Repo::open(&root).ok()?.bytes_at(rev, &file)?,
                        None => std::fs::read(root.join(&file)).ok()?,
                    };
                    if artoo_core::files::binary(&bytes) {
                        let size = Some(bytes.len() as u64);
                        return Some((Vec::new(), Vec::new(), at, None, size));
                    }
                    let text = String::from_utf8_lossy(&bytes);
                    let spans = highlight::highlight(&file, &text);
                    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                    Some((lines, spans, at, None, None))
                })
                .await;
            this.update(cx, |rv, cx| {
                rv.file_reading = None;
                cx.emit(match read {
                    Some((lines, spans, at, image, binary)) => {
                        let text: Vec<_> = lines
                            .iter()
                            .enumerate()
                            .map(|(n, l)| widen(l, spans.get(n).map_or(&[][..], Vec::as_slice)))
                            .collect();
                        let plain = text.iter().map(|(l, _)| l.to_string()).collect();
                        rv.file_view = Some(FileView {
                            path,
                            at,
                            rev,
                            lines,
                            text: Rc::new(text),
                            plain: Rc::new(std::cell::RefCell::new(plain)),
                            list: ListState::new(0, ListAlignment::Top, px(400.)),
                            rendered: true,
                            image,
                            binary,
                        });
                        ReviewEvent::FileRead
                    }
                    None => ReviewEvent::Failed(format!("{path} is not in this version.")),
                });
            })
            .ok();
        }));
    }

    /// The file's declarations, by keyword: (1-based line, depth, kind, name) — its Outline (#76).
    pub fn file_outline(&self) -> Vec<(u32, usize, &'static str, String)> {
        let Some(view) = self.file_view.as_ref().filter(|v| v.image.is_none()) else {
            return Vec::new();
        };
        view.lines
            .iter()
            .enumerate()
            .filter_map(|(n, line)| {
                let (kind, name) = artoo_core::symbols::declaration(line)?;
                let indent = line.len() - line.trim_start().len();
                Some((n as u32 + 1, indent / 4, kind, name))
            })
            .collect()
    }
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
