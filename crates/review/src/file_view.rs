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
    /// Its lines (0-based) that differ from HEAD, read from the working tree (#206): the gutter and the minimap mark
    /// them.
    pub changed: Rc<std::collections::HashSet<usize>>,
    /// The rows as last drawn, its notes among its lines: where a line's row is.
    pub shown: std::cell::RefCell<Rc<Vec<Row>>>,
    /// Its declarations with the lines they span: the scope line over its lines names those holding the top one
    /// (#207).
    pub scopes: Rc<Vec<artoo_core::symbols::Scope>>,
    /// A Markdown file: shown rendered, until Source.
    pub rendered: bool,
    /// An image file, decoded.
    pub image: Option<Arc<gpui::Image>>,
    /// A binary file that is no image: its size, and nothing to show (#78).
    pub binary: Option<u64>,
}

/// The file being read for the view.
pub type Reading = Option<Task<()>>;

/// A row of the file's list: a line, a note under it, the note being written, or a name's declarations (#207).
#[derive(Clone, Copy)]
pub enum Row {
    Line(usize),
    Note(u64),
    Composer,
    Peek,
}

/// What the lines of the file at `path` select as, one text across its rows (#93): where a caret in it is.
pub fn selects_as(path: &str) -> String {
    format!("file-lines-{path}")
}

/// `rows` with the peek right under line `line` (0-based), when one is open.
fn with_peek(mut rows: Vec<Row>, line: Option<usize>) -> Vec<Row> {
    if let Some(line) = line
        && let Some(at) = rows
            .iter()
            .position(|r| matches!(r, Row::Line(n) if *n == line))
    {
        rows.insert(at + 1, Row::Peek);
    }
    rows
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

pub use artoo_review_api::FileNotes;

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
            out.push_str(&" ".repeat(artoo_core::rows::tab_width()));
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

/// The lines (0-based) of `now`, the working tree's `path`, that differ from HEAD's: all of them for a file HEAD has
/// not (#206).
fn changed_since_head(
    root: &std::path::Path,
    path: &str,
    now: &[u8],
) -> std::collections::HashSet<usize> {
    let mut changed = std::collections::HashSet::new();
    let Ok(repo) = git2::Repository::open(root) else {
        return changed;
    };
    let old = (repo.head().ok())
        .and_then(|h| h.peel_to_tree().ok())
        .and_then(|tree| tree.get_path(std::path::Path::new(path)).ok())
        .and_then(|entry| repo.find_blob(entry.id()).ok())
        .map(|blob| blob.content().to_vec());
    let Some(old) = old else {
        let lines = String::from_utf8_lossy(now).lines().count();
        return (0..lines).collect();
    };
    let Ok(patch) = git2::Patch::from_buffers(&old, None, now, None, None) else {
        return changed;
    };
    for h in 0..patch.num_hunks() {
        let lines = patch.num_lines_in_hunk(h).unwrap_or(0);
        for l in 0..lines {
            if let Ok(line) = patch.line_in_hunk(h, l)
                && line.origin() == '+'
                && let Some(n) = line.new_lineno()
            {
                changed.insert(n as usize - 1);
            }
        }
    }
    changed
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
                        return Some((
                            Vec::new(),
                            Vec::new(),
                            at,
                            Some(Arc::new(image)),
                            None,
                            Default::default(),
                            Vec::new(),
                        ));
                    }
                    let bytes = match rev {
                        Some(rev) => Repo::open(&root).ok()?.bytes_at(rev, &file)?,
                        None => std::fs::read(root.join(&file)).ok()?,
                    };
                    if artoo_core::files::binary(&bytes) {
                        let size = Some(bytes.len() as u64);
                        return Some((
                            Vec::new(),
                            Vec::new(),
                            at,
                            None,
                            size,
                            Default::default(),
                            Vec::new(),
                        ));
                    }
                    let text = String::from_utf8_lossy(&bytes);
                    let spans = highlight::highlight(&file, &text);
                    let lines: Vec<String> = text.lines().map(str::to_owned).collect();
                    let changed = match rev {
                        None => changed_since_head(&root, &file, &bytes),
                        Some(_) => Default::default(),
                    };
                    let scopes = artoo_core::symbols::scopes(&lines);
                    Some((lines, spans, at, None, None, changed, scopes))
                })
                .await;
            this.update(cx, |rv, cx| {
                rv.file_reading = None;
                cx.emit(match read {
                    Some((lines, spans, at, image, binary, changed, scopes)) => {
                        rv.peek = None;
                        rv.file_line = None;
                        rv.file_target = None;
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
                            changed: Rc::new(changed),
                            shown: Default::default(),
                            scopes: Rc::new(scopes),
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
    /// Line `line` (0-based) of the file read comes into view, a few lines under the top (#206).
    pub fn scroll_file_to(&mut self, line: usize) {
        let Some(view) = &self.file_view else {
            return;
        };
        let shown = view.shown.borrow().clone();
        let row = (shown.iter())
            .position(|r| matches!(r, Row::Line(n) if *n == line))
            .unwrap_or(line);
        view.list.scroll_to(gpui::ListOffset {
            item_ix: row.saturating_sub(3),
            offset_in_item: px(0.),
        });
    }

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
        let peek = self.peek.as_ref().map(|p| p.line);
        self.file_target = (line as usize).checked_sub(1);
        let Some(view) = self.file_view.as_mut() else {
            return;
        };
        view.rendered = false;
        let rows = with_peek(rows(&view.lines, notes), peek);
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
        with_peek(rows(&view.lines, notes), self.peek.as_ref().map(|p| p.line))
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
        typing: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> Option<(gpui::Div, AnyElement)> {
        let view = self.file_view.as_ref()?;
        let finding = typing == Some(crate::find::INPUT);
        let head = self.render_file_head(view, typing == Some(crate::names::LINE_INPUT), cx);
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
            // ⌘F's bar under the head, the lines, and the minimap at their right (#206).
            let this = cx.entity().downgrade();
            let frame = self.frame.as_ref().and_then(std::rc::Weak::upgrade);
            let bar = (self.render_find_bar(finding, this.clone())).map(|bar| match &frame {
                Some(frame) => artoo_shell_api::input_box(bar, crate::find::INPUT, frame),
                None => bar,
            });
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .children(bar)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(self.render_file_lines(
                                    view,
                                    host,
                                    notes,
                                    selecting,
                                    this.clone(),
                                ))
                                .children(self.render_scope(view, this.clone())),
                        )
                        .child(self.render_minimap(view, this)),
                )
                .into_any_element()
        };
        Some((head, body))
    }

    /// The file's line over it: its path, language, lines and revision; Rendered | Source for a
    /// Markdown file; read only.
    fn render_file_head(
        &self,
        view: &FileView,
        line_typing: bool,
        cx: &Context<Self>,
    ) -> gpui::Div {
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
            // Its path is the bar's, its name the tab's (A, 2026-10-23). Its lines' count goes to a line (⌃G, #207).
            .child(
                div()
                    .id("file-facts")
                    .text_color(theme::faint())
                    .when(self.file_lines_shown(), |s| {
                        s.cursor_pointer()
                            .hover(|s| s.text_color(theme::text()))
                            .tooltip_show_delay(artoo_ui_kit::hover::LONG)
                            .tooltip(|_, cx| {
                                cx.new(|_| artoo_ui_kit::hover::Tip("Go to line  ⌃G"))
                                    .into()
                            })
                            .on_click(cx.listener(|rv, _: &ClickEvent, _, cx| {
                                rv.open_file_line();
                                cx.emit(ReviewEvent::Changed);
                            }))
                    })
                    .child(facts),
            )
            .child(div().flex_1())
            .children(self.render_file_line(line_typing).map(|field| {
                match self.frame.as_ref().and_then(std::rc::Weak::upgrade) {
                    Some(frame) => {
                        artoo_shell_api::input_box(field, crate::names::LINE_INPUT, &frame)
                    }
                    None => field,
                }
            }))
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

    /// The scope line over the lines (#207): the declarations holding the top line, the outermost first, once their own
    /// line has scrolled off; a click goes to one.
    fn render_scope(&self, view: &FileView, this: WeakEntity<Review>) -> Option<gpui::Div> {
        let top = view.list.logical_scroll_top();
        if top.item_ix == 0 && top.offset_in_item <= px(1.) {
            return None;
        }
        let shown = view.shown.borrow().clone();
        let line = (shown.iter().skip(top.item_ix)).find_map(|r| match r {
            Row::Line(n) => Some(*n as u32 + 1),
            _ => None,
        })?;
        let held = artoo_core::symbols::scopes_at(&view.scopes, line);
        if held.is_empty() {
            return None;
        }
        let size = self.code_size();
        let digits = view.lines.len().max(1).to_string().len() as f32;
        let left = digits * size * 0.62 + 42. + 3. + 8.;
        let parts = held
            .iter()
            .rev()
            .take(3)
            .rev()
            .enumerate()
            .map(|(i, scope)| {
                let (go, start) = (this.clone(), scope.start as usize - 1);
                div()
                    .id(("scope", i))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::accent()))
                    .when(i > 0, |s| {
                        s.child(div().text_color(theme::faint()).child("›"))
                    })
                    .child(div().text_color(theme::muted()).child(scope.kind))
                    .child(scope.name.clone())
                    .on_click(move |_, _, cx| {
                        go.update(cx, |rv, cx| {
                            rv.file_target = Some(start);
                            rv.scroll_file_to(start);
                            cx.emit(ReviewEvent::Changed);
                        })
                        .ok();
                    })
            });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                // Over the list's own 6 px at its top too: nothing shows above it.
                .h(px((size * 1.55).round() + 8.))
                .pt(px(6.))
                .pl(px(left))
                .flex()
                .items_center()
                .gap(px(6.))
                .bg(theme::panel())
                .border_b_1()
                .border_color(theme::island_border())
                .font_family(theme::code_font())
                .text_size(px(size))
                .text_color(theme::code_text())
                .children(parts),
        )
    }

    /// The minimap (#206): the file drawn small — each line a bar as long as its text —, the lines that differ from
    /// HEAD in its gutter, the find's hits over it (the current one stronger), and a slider over what is on screen,
    /// shown on hover. A click or a drag scrolls there.
    fn render_minimap(&self, view: &FileView, this: WeakEntity<Review>) -> AnyElement {
        const BUCKETS: usize = 240;
        const WIDTH: f32 = 72.;
        let total = view.lines.len().max(1);
        let per = total.div_ceil(BUCKETS).max(1);
        let buckets = total.div_ceil(per);
        let share = |n: usize| n as f32 / total as f32;
        let mut shape = Vec::with_capacity(buckets);
        for b in 0..buckets {
            let lines = &view.lines[b * per..((b + 1) * per).min(view.lines.len())];
            let longest = lines.iter().map(|l| l.trim_end().len()).max().unwrap_or(0);
            let indent = (lines.iter())
                .map(|l| l.len() - l.trim_start().len())
                .min()
                .unwrap_or(0);
            shape.push((indent.min(60), longest.min(100)));
        }
        let band = 1. / buckets as f32;
        let bars = shape
            .into_iter()
            .enumerate()
            .filter(|(_, (_, w))| *w > 0)
            .map(|(b, (indent, w))| {
                div()
                    .absolute()
                    .top(gpui::relative(b as f32 * band))
                    .h(gpui::relative(band * 0.7))
                    .left(px(8. + indent as f32 * 0.5))
                    .w(px((w.saturating_sub(indent)) as f32 * 0.55))
                    .bg(theme::faint())
                    .opacity(0.35)
            });
        let changed = (view.changed.iter()).map(|&n| {
            div()
                .absolute()
                .top(gpui::relative(share(n)))
                .h(px(2.))
                .left_0()
                .w(px(3.))
                .bg(theme::added())
        });
        let current = self.file_matches.get(self.match_at).copied();
        let hits = self.file_matches.iter().map(|&n| {
            div()
                .absolute()
                .top(gpui::relative(share(n)))
                .h(px(2.))
                .left(px(6.))
                .right_0()
                .bg(if Some(n) == current {
                    theme::warning()
                } else {
                    theme::warning_bg()
                })
        });
        // What is on screen: the rows from the list's top, as many as fit.
        let top = view.list.logical_scroll_top().item_ix;
        let seen = (f32::from(view.list.viewport_bounds().size.height) / (self.code_size() * 1.55))
            .max(1.);
        let (from, span) = (share(top.min(total)), (seen / total as f32).min(1.));
        let list = view.list.clone();
        let rows = view.shown.borrow().len().max(total);
        let scroll = move |y: f32, bounds: gpui::Bounds<gpui::Pixels>| {
            let at =
                ((y - f32::from(bounds.origin.y)) / f32::from(bounds.size.height)).clamp(0., 1.);
            let row = (at * rows as f32) as usize;
            list.scroll_to(gpui::ListOffset {
                item_ix: row.saturating_sub((seen / 2.) as usize),
                offset_in_item: px(0.),
            });
        };
        let (down, drag) = (scroll.clone(), scroll);
        let (that, those) = (this.clone(), this);
        // Where the minimap is, read as it is laid out: a click's place in it.
        let place: Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>> = Rc::default();
        let (keep, at_down, at_drag) = (place.clone(), place.clone(), place);
        div()
            .id("minimap")
            .group("minimap")
            .relative()
            .flex_none()
            .w(px(WIDTH))
            .h_full()
            .border_l_1()
            .border_color(theme::island_border())
            .cursor_pointer()
            .children(bars)
            .children(changed)
            .children(hits)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(gpui::relative(from))
                    .h(gpui::relative(span))
                    .bg(theme::hover())
                    .invisible()
                    .group_hover("minimap", |s| s.visible()),
            )
            .child(
                gpui::canvas(move |bounds, _, _| keep.set(bounds), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .on_mouse_down(gpui::MouseButton::Left, move |ev, _, cx| {
                down(f32::from(ev.position.y), at_down.get());
                that.update(cx, |_, cx| cx.notify()).ok();
            })
            .on_mouse_move(move |ev, _, cx| {
                if ev.pressed_button == Some(gpui::MouseButton::Left) {
                    drag(f32::from(ev.position.y), at_drag.get());
                    those.update(cx, |_, cx| cx.notify()).ok();
                }
            })
            .into_any_element()
    }

    /// The lines with their numbers; a number's click writes a note on its line.
    fn render_file_lines(
        &self,
        view: &FileView,
        host: Rc<dyn Host>,
        notes: &FileNotes,
        selecting: Selecting,
        this: WeakEntity<Review>,
    ) -> AnyElement {
        let size = self.code_size();
        let row = (size * 1.55).round();
        let digits = view.lines.len().max(1).to_string().len() as f32;
        // Room on its left for the + that writes a note (#104).
        let gutter = digits * size * 0.62 + 42.;
        let rows = Rc::new(with_peek(
            rows(&view.lines, notes),
            self.peek.as_ref().map(|p| p.line),
        ));
        let peek = (self.peek.as_ref()).map(|p| Rc::new(crate::names::PeekLook::of(p, &view.path)));
        let target = self.file_target;
        *view.shown.borrow_mut() = rows.clone();
        let (terms, current) = (
            self.dterms.clone(),
            self.file_matches.get(self.match_at).copied(),
        );
        let changed = view.changed.clone();
        if view.list.item_count() != rows.len() {
            let top = view.list.logical_scroll_top();
            view.list.reset(rows.len());
            view.list.scroll_to(top);
        }
        let text = view.text.clone();
        // Its lines select as one text, across rows (#93).
        let (sel, note, plain) = (
            selecting,
            SharedString::from(selects_as(&view.path)),
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
                Row::Peek => {
                    return match &peek {
                        Some(look) => crate::names::peek_card(look, gutter + 11., this.clone()),
                        None => div().into_any_element(),
                    };
                }
            };
            let (line, spans) = &text[n];
            // The find's hits tinted, the current line's filled (#206).
            let found = terms.ranges(line);
            let strong = current == Some(n);
            let highlights = artoo_core::rows::runs(line, spans, &[], &found, None)
                .into_iter()
                .filter(|r| r.range.end <= line.len())
                .map(|run| {
                    let style = HighlightStyle {
                        color: run.color.map(theme::code),
                        font_style: run.italic.then_some(FontStyle::Italic),
                        font_weight: run.bold.then_some(gpui::FontWeight::SEMIBOLD),
                        ..Default::default()
                    };
                    let style = match (run.found, strong) {
                        (true, true) => HighlightStyle {
                            color: Some(theme::base().into()),
                            background_color: Some(theme::warning().into()),
                            ..style
                        },
                        (true, false) => HighlightStyle {
                            background_color: Some(theme::warning_bg().into()),
                            ..style
                        },
                        _ => style,
                    };
                    (run.range, style)
                })
                .collect::<Vec<_>>();
            let is_changed = changed.contains(&n);
            let comment = host.clone();
            let styled = StyledText::new(line.clone()).with_highlights(highlights);
            let layout = styled.layout().clone();
            // ⌘-click on a name: its declarations (#207).
            let (by_name, at_layout, name_note) = (this.clone(), layout.clone(), note.clone());
            let plus = host.clone();
            // In the code's font and size on each row: the list's own reach none of its rows.
            div()
                .id(("file-row", n))
                .group("file-row")
                .relative()
                .w_full()
                .h(px(row))
                .font_family(theme::code_font())
                .text_size(px(size))
                .flex()
                .items_center()
                .when(target == Some(n), |s| s.bg(theme::hover()))
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
                // A line that differs from HEAD: a bar beside its number (#206).
                .child(
                    div()
                        .flex_none()
                        .w(px(3.))
                        .h_full()
                        .when(is_changed, |s| s.bg(theme::added())),
                )
                .child(
                    div()
                        .flex_1()
                        .pl_2()
                        .whitespace_nowrap()
                        .text_color(theme::code_text())
                        .on_mouse_down(gpui::MouseButton::Left, move |ev, _, cx| {
                            if !ev.modifiers.platform {
                                return;
                            }
                            let at = artoo_ui_kit::input::nearest_index(&at_layout, ev.position);
                            (by_name.update(cx, |rv, cx| rv.peek_name(&name_note, n, at, cx))).ok();
                        })
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
        let said = |rows: Vec<Row>| -> Vec<String> {
            (rows.iter())
                .map(|r| match r {
                    Row::Line(n) => format!("line {n}"),
                    Row::Note(id) => format!("note {id}"),
                    Row::Composer => "composer".to_owned(),
                    Row::Peek => "peek".to_owned(),
                })
                .collect()
        };
        assert_eq!(
            said(rows(&lines, &notes)),
            ["line 0", "note 3", "line 1", "note 7", "composer", "line 2"]
        );
        // A name's declarations open right under its line, over its notes (#207).
        assert_eq!(
            said(with_peek(rows(&lines, &notes), Some(1))),
            [
                "line 0", "note 3", "line 1", "peek", "note 7", "composer", "line 2"
            ]
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
