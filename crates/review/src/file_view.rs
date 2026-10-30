//! A file read in Code: what it was read at, its lines highlighted with their tabs widened, a Markdown file
//! rendered until Source, an image decoded, a binary file's size; and where a note on a line sits now.

use crate::review::{FileEvent, Host, Review, ReviewEvent};
use artoo_core::git::{ImageKind, Repo};
use artoo_core::highlight::{self, Span};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{ROW_RADIUS, format, icons, keys, px, theme};
use git2::Oid;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontStyle, HighlightStyle, ListAlignment,
    ListState, SharedString, StyledText, Task, WeakEntity, div, img, list, prelude::*,
};
use std::path::PathBuf;
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

/// What other features put on the file's lines: the agents' notes, each its id, the line it was written on and
/// that line's text then, and the line of the note being written.
#[derive(Default)]
pub struct FileNotes {
    pub notes: Vec<(u64, u32, String)>,
    pub compose: Option<u32>,
}

/// The rows of a file of `lines`: its lines, each with the notes on it and the note being written.
pub fn rows(lines: &[String], notes: &FileNotes) -> Vec<Row> {
    let mut placed: Vec<(usize, u64)> = notes
        .notes
        .iter()
        .filter_map(|(id, line, code)| Some((place_note(lines, *line, code)?, *id)))
        .collect();
    placed.sort_unstable();
    let compose = notes.compose.map(|line| line as usize - 1);
    let mut out = Vec::with_capacity(lines.len() + placed.len() + 1);
    let mut next = placed.iter().peekable();
    for n in 0..lines.len() {
        out.push(Row::Line(n));
        while let Some(&&(at, id)) = next.peek() {
            if at != n {
                break;
            }
            out.push(Row::Note(id));
            next.next();
        }
        if compose == Some(n) {
            out.push(Row::Composer);
        }
    }
    out
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

    /// The file's lines are on screen: neither an image nor a document rendered.
    pub fn file_lines_shown(&self) -> bool {
        self.file_view
            .as_ref()
            .is_some_and(|v| v.image.is_none() && !(v.markdown() && v.rendered))
    }

    /// The file, a document, shown rendered or as its source.
    pub fn set_rendered(&mut self, on: bool, cx: &mut Context<Self>) {
        if let Some(view) = &mut self.file_view {
            view.rendered = on;
        }
        cx.emit(ReviewEvent::Changed);
    }

    /// Line `line` of the file comes into view, its source shown; `notes` are what is on its lines.
    pub fn reveal_file_line(&mut self, line: u32, notes: &FileNotes) {
        let Some(view) = self.file_view.as_mut() else {
            return;
        };
        view.rendered = false;
        let rows = rows(&view.lines, notes);
        let fresh = view.list.item_count() != rows.len();
        if fresh {
            view.list.reset(rows.len());
        }
        if let Some(ix) = rows
            .iter()
            .position(|r| matches!(r, Row::Line(n) if *n + 1 == line as usize))
        {
            // Rows not measured yet (a file just read) cannot be revealed: the line goes near the
            // top, a few above it showing (#154).
            if fresh {
                let item_ix = ix.saturating_sub(4);
                view.list.scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.),
                });
            } else {
                view.list.scroll_to_reveal_item(ix);
            }
        }
    }

    /// The line at the top of `path`'s view as it is scrolled, `notes` on its lines: where Open in opens it
    /// (#78).
    pub fn file_top_line(&self, path: &str, notes: &FileNotes) -> u32 {
        let Some(view) = self.file_view.as_ref().filter(|v| v.path == path) else {
            return 1;
        };
        let top = view.list.logical_scroll_top().item_ix;
        rows(&view.lines, notes)
            .iter()
            .take(top + 1)
            .rev()
            .find_map(|row| match row {
                Row::Line(n) => Some(*n as u32 + 1),
                _ => None,
            })
            .unwrap_or(1)
    }

    /// The file's head line and what is under it: its lines with `notes` on them (the threads and the
    /// composer from `host`), a document rendered, an image, or `binary` for a file that is neither. Its text
    /// selects with `selecting`.
    pub fn render_file_view(
        &self,
        host: Rc<dyn Host>,
        notes: &FileNotes,
        binary: Option<AnyElement>,
        selecting: Selecting,
        cx: &mut Context<Self>,
    ) -> Option<(gpui::Div, AnyElement)> {
        let view = self.file_view.as_ref()?;
        let head = self.render_file_head(view, cx);
        let body = if let Some(binary) = binary.filter(|_| view.binary.is_some()) {
            binary
        } else if let Some(image) = &view.image {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24.))
                .child(img(image.clone()).max_w_full().max_h_full())
                .into_any_element()
        } else if view.markdown() && view.rendered {
            div()
                .id("file-rendered")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(24.))
                .py(px(18.))
                .text_size(px(13.))
                .child(markdown::body(
                    &view.lines.join("\n"),
                    true,
                    format!("file-{}", view.path),
                    None,
                    Some(selecting),
                    cx,
                ))
                .into_any_element()
        } else {
            self.render_file_lines(view, host, notes, selecting)
        };
        Some((head, body))
    }

    /// The file's line over it: its path, language, lines and revision; Rendered | Source for a
    /// Markdown file; read only.
    fn render_file_head(&self, view: &FileView, cx: &Context<Self>) -> gpui::Div {
        let markdown = view.markdown() && view.image.is_none();
        let facts = if view.image.is_some() {
            format!("image · {}", view.at)
        } else if let Some(size) = view.binary {
            format!("{} · {}", crate::image_view::bytes(size as usize), view.at)
        } else {
            format!("{} lines · {}", view.lines.len(), view.at)
        };
        div()
            .flex_none()
            .h(px(34.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(theme::island_border())
            .text_size(px(12.))
            // Its path is the bar's, its name the tab's (A, 2026-10-23).
            .child(div().text_color(theme::faint()).child(facts))
            .child(div().flex_1())
            // `c` asks which line the note goes on.
            .children(
                self.goline
                    .as_ref()
                    .filter(|_| self.file_lines_shown())
                    .map(|typed| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_color(theme::agent())
                            .child("Note on line")
                            .child(
                                div()
                                    .min_w(px(28.))
                                    .px(px(6.))
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(theme::agent())
                                    .child(if typed.is_empty() {
                                        " ".to_owned()
                                    } else {
                                        typed.clone()
                                    }),
                            )
                            .child(keys::caps("↵", keys::On::Plain))
                    }),
            )
            .children(markdown.then(|| {
                let rendered = view.rendered;
                artoo_ui_kit::controls::group()
                    .child(
                        artoo_ui_kit::controls::chip("file-rendered-on", "Rendered", rendered)
                            .on_click(
                                cx.listener(|rv, _: &ClickEvent, _, cx| rv.set_rendered(true, cx)),
                            ),
                    )
                    .child(
                        artoo_ui_kit::controls::chip("file-source", "Source", !rendered).on_click(
                            cx.listener(|rv, _: &ClickEvent, _, cx| rv.set_rendered(false, cx)),
                        ),
                    )
            }))
            .child(div().text_color(theme::faint()).child("read only"))
    }

    /// The lines with their numbers; a number's click writes a note on its line.
    fn render_file_lines(
        &self,
        view: &FileView,
        host: Rc<dyn Host>,
        notes: &FileNotes,
        selecting: Selecting,
    ) -> AnyElement {
        let size = self.code_size();
        let row = (size * 1.55).round();
        let digits = view.lines.len().max(1).to_string().len() as f32;
        // Room on its left for the + that writes a note (#104).
        let gutter = digits * size * 0.62 + 42.;
        let rows = Rc::new(rows(&view.lines, notes));
        if view.list.item_count() != rows.len() {
            let top = view.list.logical_scroll_top();
            view.list.reset(rows.len());
            view.list.scroll_to(top);
        }
        let text = view.text.clone();
        // Its lines select as one text, across rows (#93).
        let (sel, note, plain) = (
            selecting,
            SharedString::from(format!("file-lines-{}", view.path)),
            view.plain.clone(),
        );
        list(view.list.clone(), move |ix, _, cx| {
            let Some(&row_kind) = rows.get(ix) else {
                return div().into_any_element();
            };
            let n = match row_kind {
                Row::Line(n) => n,
                Row::Note(id) => {
                    return host.pin(artoo_core::rows::Row::Thread(id), true, gutter + 8., cx);
                }
                Row::Composer => {
                    return host.pin(artoo_core::rows::Row::Composer, true, gutter + 8., cx);
                }
            };
            let (line, spans) = &text[n];
            let highlights = spans
                .iter()
                .filter(|s| s.range.end <= line.len())
                .map(|s| {
                    (
                        s.range.clone(),
                        HighlightStyle {
                            color: Some(theme::code(s.color)),
                            font_style: s.italic.then_some(FontStyle::Italic),
                            font_weight: s.bold.then_some(gpui::FontWeight::SEMIBOLD),
                            ..Default::default()
                        },
                    )
                })
                .collect::<Vec<_>>();
            let comment = host.clone();
            let styled = StyledText::new(line.clone()).with_highlights(highlights);
            let layout = styled.layout().clone();
            let plus = host.clone();
            // In the code's font and size on each row: the list's own reach none of its rows.
            div()
                .id(("file-row", n))
                .group("file-row")
                .relative()
                .h(px(row))
                .font_family(theme::code_font())
                .text_size(px(size))
                .flex()
                .items_center()
                // A note for your agent on the line, as the diff's + is (#104).
                .child(
                    div()
                        .id(("file-plus", n))
                        .absolute()
                        .left(px(3.))
                        .top(px(((row - 18.) / 2.).max(0.)))
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(ROW_RADIUS))
                        .bg(theme::agent())
                        .text_color(theme::base())
                        .font_family(theme::ui_font())
                        .text_size(px(13.))
                        .cursor_pointer()
                        .invisible()
                        .group_hover("file-row", |s| s.visible())
                        .child(icons::icon("plus").size(px(13.)).text_color(theme::base()))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            plus.file(FileEvent::Note(n as u32 + 1), cx);
                        }),
                )
                .child(
                    div()
                        .id(("file-line", n))
                        .w(px(gutter))
                        .flex_none()
                        .flex()
                        .justify_end()
                        .pr_2()
                        .text_color(theme::muted())
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::agent()))
                        .child((n + 1).to_string())
                        .on_click(move |_, _, cx| comment.file(FileEvent::Note(n as u32 + 1), cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .pl_2()
                        .whitespace_nowrap()
                        .text_color(theme::code_text())
                        .child(markdown::line_block(
                            styled.into_any_element(),
                            layout,
                            note.clone(),
                            n,
                            plain.clone(),
                            sel.clone(),
                        )),
                )
                .into_any_element()
        })
        .flex_1()
        .py(px(6.))
        .font_family(theme::code_font())
        .text_size(px(size))
        .into_any_element()
    }
}

/// What a page asks of `review` for the file it reads (`artoo_review_api::FileView`), as the tab offers it.
pub fn file_view(review: &Entity<Review>) -> Rc<dyn artoo_review_api::FileView> {
    Rc::new(FileViewImpl(review.downgrade()))
}

struct FileViewImpl(WeakEntity<Review>);

impl artoo_review_api::FileView for FileViewImpl {
    fn read(&self, root: PathBuf, path: String, rev: Option<Oid>, cx: &mut App) {
        self.0
            .update(cx, |rv, cx| rv.read_file(root, path, rev, cx))
            .ok();
    }

    fn path(&self, cx: &App) -> Option<String> {
        Some(self.0.upgrade()?.read(cx).file_view.as_ref()?.path.clone())
    }

    fn close(&self, cx: &mut App) {
        self.0.update(cx, |rv, _| rv.file_view = None).ok();
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
    fn a_files_rows_put_its_notes_and_the_composer_under_their_lines() {
        let lines: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let notes = FileNotes {
            notes: vec![
                (7, 2, "b".to_owned()),
                (3, 1, "a".to_owned()),
                (9, 2, "gone".to_owned()),
            ],
            compose: Some(2),
        };
        let rows: Vec<String> = rows(&lines, &notes)
            .iter()
            .map(|r| match r {
                Row::Line(n) => format!("line {n}"),
                Row::Note(id) => format!("note {id}"),
                Row::Composer => "composer".to_owned(),
            })
            .collect();
        assert_eq!(
            rows,
            ["line 0", "note 3", "line 1", "note 7", "composer", "line 2"]
        );
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
