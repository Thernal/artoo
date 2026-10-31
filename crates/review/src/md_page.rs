//! A changed markdown file read as a page (Design #11 A + B, `docs/specs/markdown-viewer.md`): its card has
//! Rendered | Source (M, the active file's), Rendered by default; a document turned to Source stays so between
//! launches (`page-source.txt`, per repository). The page follows the toolbar's Split | Unified. Unified draws the
//! new version with the changes in it — a bar in the margin for every changed block, green added, yellow changed
//! with its words marked, red removed (faded and struck; a removed section folds to one line). Split draws the old
//! version beside the new, block beside block, a hatched gap where one side has none. Images come from the
//! repository at each side's version (`FileDiff::pictures`). A comment on a block goes to the source line it starts
//! on, written under the block on the page; its count opens the block's comments there. Viewed one file at a time
//! the page scrolls on its own: n / p step through its changed blocks, space and ⇧space scroll it. What goes under
//! a block, the marks on its lines and Split's heads come from the tab in a `PageLook`; a click goes back to it
//! (`PageEvent`).

use crate::diff_view::LineNote;
use crate::review::{Review, ReviewEvent};
use artoo_core::git::FileDiff;
use artoo_core::markdown::Block;
use artoo_core::mddiff::{self, Change, Row};
use artoo_core::{markdown, storage};
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::markdown::{DocPictures, Selecting, doc_block};
use artoo_ui_kit::{hover, icons, keys, motion, px, theme};
use gpui::{AnyElement, App, ClickEvent, FontWeight, Rgba, WeakEntity, div, prelude::*};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// A document's rows, by the texts compared (their addresses).
type Rows = RefCell<HashMap<(usize, usize), Rc<Vec<Row>>>>;

const SOURCE_FILE: &str = "page-source.txt";

/// How changed documents are read, and their rows kept between frames.
#[derive(Default)]
pub struct Page {
    /// The documents read as Source (the ordinary diff of the text), by path; of the repository
    /// `source_of`.
    source: HashSet<String>,
    source_of: Option<String>,
    /// Removed sections opened, by path and the first old line.
    pub open: HashSet<(String, u32)>,
    /// Blocks whose comments are shown under them: path, side, first line.
    pub notes_open: HashSet<(String, bool, u32)>,
    /// A document's rows, by the texts compared.
    cache: Rows,
    /// One file at a time: the page's own scroll, its children the blocks.
    pub scroll: gpui::ScrollHandle,
    /// The changed blocks among those children, as last drawn; the one n / p last went to.
    pub changes: RefCell<Vec<usize>>,
    pub at: Option<usize>,
}

/// The paths of repository `repo` in the lines of `page-source.txt` (`repo<TAB>path`).
fn source_paths(text: &str, repo: &str) -> HashSet<String> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(r, _)| *r == repo)
        .map(|(_, p)| p.to_owned())
        .collect()
}

/// `text` with repository `repo`'s lines replaced by `paths`; other repositories' kept.
fn with_source_paths(text: &str, repo: &str, paths: &HashSet<String>) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| l.split_once('\t').is_some_and(|(r, _)| r != repo))
        .map(str::to_owned)
        .chain(paths.iter().map(|p| format!("{repo}\t{p}")))
        .collect();
    lines.sort_unstable();
    lines.join("\n") + "\n"
}

impl Page {
    /// Another file: n / p start again from the top of its view.
    pub fn leave(&mut self) {
        self.at = None;
    }

    /// Document `path` is read as Source.
    pub fn is_source(&self, path: &str) -> bool {
        self.source.contains(path)
    }

    /// The documents of repository `repo` read as Source, once per repository.
    pub fn load(&mut self, repo: &str) {
        if self.source_of.as_deref() != Some(repo) {
            self.source = source_paths(&storage::read(SOURCE_FILE), repo);
            self.source_of = Some(repo.to_owned());
        }
    }

    /// Document `path` read as Source or not; remembered.
    pub fn set_source(&mut self, path: &str, source: bool) -> bool {
        let changed = if source {
            self.source.insert(path.to_owned())
        } else {
            self.source.remove(path)
        };
        if changed && let Some(repo) = &self.source_of {
            storage::save(
                SOURCE_FILE,
                with_source_paths(&storage::read(SOURCE_FILE), repo, &self.source),
            );
        }
        changed
    }

    pub fn rows(&self, file: &FileDiff) -> Rc<Vec<Row>> {
        let key = |t: &Option<std::sync::Arc<str>>| {
            t.as_ref()
                .map_or(0, |t| std::sync::Arc::as_ptr(t) as *const u8 as usize)
        };
        let key = (key(&file.old_text), key(&file.new_text));
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 16 {
            cache.clear();
        }
        cache
            .entry(key)
            .or_insert_with(|| {
                Rc::new(mddiff::diff(
                    file.old_text.as_deref().unwrap_or_default(),
                    file.new_text.as_deref().unwrap_or_default(),
                ))
            })
            .clone()
    }
}

/// The margin bar's colour for a change.
pub fn bar(change: Change) -> Option<Rgba> {
    match change {
        Change::Same => None,
        Change::Added => Some(theme::added()),
        Change::Changed => Some(theme::warning()),
        Change::Removed => Some(theme::removed()),
    }
}

/// The section a run of removed blocks starts: its heading, when it starts with one.
pub fn heading(block: &Block) -> Option<String> {
    match block {
        Block::Heading(_, i) => Some(i.text.clone()),
        _ => None,
    }
}

impl Review {
    /// Whether file `file` is drawn as a page (not as its text diff).
    pub fn shows_page(&self, file: &FileDiff) -> bool {
        !self.page.is_source(file.path())
            && markdown::is_document(file.path())
            && file.note.is_none()
            && !file.hunks.is_empty()
            && (file.old_text.is_some() || file.new_text.is_some())
    }

    /// One file at a time, and it is a page.
    pub fn page_alone(&self) -> bool {
        self.stack.is_none()
            && self
                .diff
                .as_ref()
                .and_then(|d| d.files.get(self.file))
                .is_some_and(|f| self.shows_page(f))
    }
}

/// A click on a document's page or its switch, for the tab to answer.
#[derive(Clone)]
pub enum PageEvent {
    /// Document `path` read Rendered (`false`) or as Source.
    Source { path: String, to: bool },
    /// A block's count: its comments shown under it on the page, or hidden again.
    Notes { ix: usize, old: bool, first: u32 },
    /// A block's +: a comment written on the line it starts on.
    Comment { ix: usize, old: bool, first: u32 },
    /// A removed section's line, by path and first old line: shown where it was, or folded again.
    Fold(String, u32),
}

/// What answers a page's clicks.
pub type OnPage = Rc<dyn Fn(PageEvent, &mut App)>;

/// What the tab gives a document's page to draw it with.
pub struct PageLook {
    /// The comment marks on its lines.
    pub notes: Vec<LineNote>,
    /// What goes under a block (the composer, its opened comments), by side and first line.
    pub under: HashMap<(bool, u32), Vec<AnyElement>>,
    /// Split's column heads: the versions compared.
    pub heads: (String, String),
    /// A block's text can be selected and copied, as a comment's.
    pub selecting: Selecting,
    /// Just turned from Source: it fades in (#56, 15).
    pub coming: bool,
    pub on: OnPage,
}

/// A page's clicks said to the tab that holds `this` (`ReviewEvent::Page`).
pub fn emitting(this: WeakEntity<Review>) -> OnPage {
    Rc::new(move |event, cx| {
        this.update(cx, |_, cx| cx.emit(ReviewEvent::Page(event)))
            .ok();
    })
}

impl Review {
    /// File `ix`'s switch (`page_switch`), its clicks said to the tab (`emitting`).
    pub fn switch_of(
        &self,
        ix: usize,
        legend: bool,
        this: WeakEntity<Review>,
    ) -> Option<AnyElement> {
        let file = self.diff.as_ref()?.files.get(ix)?;
        self.page_switch(file, legend, emitting(this))
    }

    /// Rendered | Source, M, and with `legend` what the bars mean while rendered, for a document's header.
    pub fn page_switch(&self, file: &FileDiff, legend: bool, on: OnPage) -> Option<AnyElement> {
        if !markdown::is_document(file.path()) || file.note.is_some() || file.hunks.is_empty() {
            return None;
        }
        let source = self.page.is_source(file.path());
        let set = |to: bool| {
            let on = on.clone();
            let path = file.path().to_owned();
            move |_: &ClickEvent, _: &mut gpui::Window, cx: &mut App| {
                cx.stop_propagation();
                on(
                    PageEvent::Source {
                        path: path.clone(),
                        to,
                    },
                    cx,
                );
            }
        };
        let key = |color: Rgba, word: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().w(px(3.)).h(px(12.)).rounded(px(2.)).bg(color))
                .child(word)
        };
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    group()
                        .child(
                            chip(
                                gpui::ElementId::Name(
                                    format!("page-rendered-{}", file.path()).into(),
                                ),
                                "Rendered",
                                !source,
                            )
                            .on_click(set(false)),
                        )
                        .child(
                            chip(
                                gpui::ElementId::Name(
                                    format!("page-source-{}", file.path()).into(),
                                ),
                                "Source",
                                source,
                            )
                            .on_click(set(true)),
                        ),
                )
                .child(keys::caps("M", keys::On::Plain))
                .children((legend && !source).then(|| {
                    div()
                        .flex()
                        .gap(px(10.))
                        .ml(px(4.))
                        .text_size(px(11.))
                        .text_color(theme::muted())
                        .child(key(theme::added(), "added"))
                        .child(key(theme::warning(), "changed"))
                        .child(key(theme::removed(), "removed"))
                }))
                .into_any_element(),
        )
    }

    /// File `ix` as a page in the list of every file: Unified or Split as the toolbar says.
    pub fn render_page(&self, ix: usize, look: PageLook, cx: &App) -> AnyElement {
        let coming = look.coming;
        let blocks = self.page_blocks(ix, look, cx);
        let page = page_text(div().w_full().py(px(16.)).pl(px(44.)).pr(px(24.))).child(
            div()
                .when(self.options.unified, |s| s.max_w(px(880.)))
                .flex()
                .flex_col()
                .gap(px(12.))
                .children(blocks.into_iter().map(|(b, _)| b)),
        );
        if coming {
            motion::appear(page, ("page-coming", ix), motion::FAST, |page, v| {
                page.opacity(v)
            })
            .into_any_element()
        } else {
            page.into_any_element()
        }
    }

    /// File `ix` alone as a page that scrolls on its own, its blocks its children, so n / p can step through the
    /// changed ones.
    pub fn render_page_alone(&self, ix: usize, look: PageLook, cx: &App) -> AnyElement {
        let blocks = self.page_blocks(ix, look, cx);
        *self.page.changes.borrow_mut() = blocks
            .iter()
            .enumerate()
            .filter(|(_, (_, changed))| *changed)
            .map(|(k, _)| k)
            .collect();
        let unified = self.options.unified;
        // Laid over its place rather than in it: its blocks never make the pane taller than the window, so it
        // scrolls.
        let page = page_text(
            div()
                .id("page")
                .track_scroll(&self.page.scroll)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(12.))
                .py(px(16.))
                .pl(px(44.))
                .pr(px(24.)),
        )
        .children(blocks.into_iter().map(|(b, _)| {
            div()
                .flex_none()
                .when(unified, |s| s.max_w(px(880.)))
                .child(b)
        }));
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(page)
            .into_any_element()
    }

    /// n / p on a page viewed alone: the next (or previous) changed block, from the last one gone to, else from
    /// the top of the view. None when no page takes the keys, else whether it went.
    pub fn step_page(&mut self, forward: bool) -> Option<bool> {
        if !self.page_alone() {
            return None;
        }
        let changes = self.page.changes.borrow().clone();
        let top = self.page.scroll.top_item();
        let from = self.page.at.filter(|&at| at + 1 >= top);
        let to = if forward {
            changes
                .iter()
                .copied()
                .find(|&c| from.map_or(c >= top, |at| c > at))
        } else {
            changes
                .iter()
                .rev()
                .copied()
                .find(|&c| c < from.unwrap_or(top))
        };
        let Some(to) = to else {
            return Some(false);
        };
        self.page.at = Some(to);
        self.page.scroll.scroll_to_top_of_item(to);
        Some(true)
    }

    /// Space and ⇧space on a page viewed alone: a screen down or up. Whether the page took it.
    pub fn scroll_page(&mut self, down: bool) -> bool {
        if !self.page_alone() {
            return false;
        }
        let s = &self.page.scroll;
        let step = s.bounds().size.height * 0.85;
        let y = if down {
            (s.offset().y - step).max(-s.max_offset().y)
        } else {
            (s.offset().y + step).min(px(0.))
        };
        s.set_offset(gpui::point(s.offset().x, y));
        self.page.at = None;
        true
    }

    /// A removed section shown where it was, or folded again.
    pub fn flip_page_fold(&mut self, key: (String, u32)) {
        if !self.page.open.remove(&key) {
            self.page.open.insert(key);
        }
        self.remeasure_pages();
    }

    /// File `ix`'s blocks: Unified or Split as the toolbar says, each with whether it is a change.
    fn page_blocks(&self, ix: usize, look: PageLook, cx: &App) -> Vec<(AnyElement, bool)> {
        let Some(file) = self.diff.as_ref().and_then(|d| d.files.get(ix)) else {
            return Vec::new();
        };
        let rows = self.page.rows(file);
        let path = file.path().to_owned();
        let side = |old: bool| {
            let pictures: DocPictures = file
                .pictures
                .iter()
                .filter(|p| p.old == old)
                .map(|p| (p.src.clone(), (self.images.image(&p.image), p.image.size)))
                .collect();
            Rc::new(pictures)
        };
        let (old_pics, new_pics) = (side(true), side(false));
        let PageLook {
            notes,
            under,
            heads,
            selecting,
            on,
            ..
        } = look;
        let ctx = BlockCtx {
            ix,
            sel: selecting,
            on,
            notes: Rc::new(notes),
            under: RefCell::new(under),
        };
        if self.options.unified {
            self.unified(&rows, &path, &ctx, &old_pics, &new_pics, cx)
        } else {
            split(&rows, heads, &ctx, &old_pics, &new_pics, cx)
        }
    }

    /// Unified's blocks, each with whether it is a change.
    fn unified(
        &self,
        rows: &[Row],
        path: &str,
        ctx: &BlockCtx,
        old_pics: &Rc<DocPictures>,
        new_pics: &Rc<DocPictures>,
        cx: &App,
    ) -> Vec<(AnyElement, bool)> {
        let mut out: Vec<(AnyElement, bool)> = Vec::new();
        let mut k = 0;
        while k < rows.len() {
            let row = &rows[k];
            // A run of removed blocks that starts a section folds to one line.
            if row.change == Change::Removed
                && let Some((first, (line, _))) = &row.old
                && heading(first).is_some()
            {
                let end = k + rows[k..]
                    .iter()
                    .take_while(|r| r.change == Change::Removed)
                    .count();
                let names: Vec<String> = rows[k..end]
                    .iter()
                    .filter_map(|r| r.old.as_ref().and_then(|(b, _)| heading(b)))
                    .collect();
                let key = (path.to_owned(), *line);
                let open = self.page.open.contains(&key);
                out.push((fold_line(&names, open, key, ctx.on.clone(), k), true));
                if open {
                    // Opened, its blocks fade in (#56, 15).
                    for (n, r) in rows[k..end].iter().enumerate() {
                        let block = motion::appear(
                            div().child(ctx.block(r, true, old_pics, k + n, true, cx)),
                            ("removed-opening", k + n),
                            motion::BASE,
                            |block, v| block.opacity(v),
                        );
                        out.push((block.into_any_element(), false));
                    }
                }
                k = end;
                continue;
            }
            let pics = if row.change == Change::Removed {
                old_pics
            } else {
                new_pics
            };
            out.push((
                ctx.block(row, row.change == Change::Removed, pics, k, true, cx),
                row.change != Change::Same,
            ));
            k += 1;
        }
        out
    }
}

/// Split's column heads (`before`, `now`), then its rows, each with whether it is a change.
fn split(
    rows: &[Row],
    (before, now): (String, String),
    ctx: &BlockCtx,
    old_pics: &Rc<DocPictures>,
    new_pics: &Rc<DocPictures>,
    cx: &App,
) -> Vec<(AnyElement, bool)> {
    let head = |text: String| {
        let (version, rest) = text.split_once(" · ").unwrap_or((&text, ""));
        div()
            .flex_1()
            .min_w_0()
            .pb(px(6.))
            .border_b_1()
            .border_color(theme::island_border())
            .text_size(px(12.))
            .text_color(theme::faint())
            .flex()
            .gap(px(4.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(version.to_owned()),
            )
            .children((!rest.is_empty()).then(|| format!("· {rest}")))
    };
    let hatch = || {
        div()
            .flex_1()
            .min_w_0()
            .min_h(px(24.))
            .mr(px(64.))
            .rounded(px(4.))
            .bg(gpui::pattern_slash(theme::selected(), 1., 6.))
            .into_any_element()
    };
    let cell = |el: AnyElement| div().flex_1().min_w_0().child(el).into_any_element();
    let heads = div()
        .flex()
        .gap(px(56.))
        .child(head(before))
        .child(head(now))
        .into_any_element();
    std::iter::once((heads, false))
        .chain(rows.iter().enumerate().map(|(k, r)| {
            let (left, right) = match r.change {
                Change::Added => (hatch(), cell(ctx.side_block(r, false, new_pics, k, cx))),
                Change::Removed => (cell(ctx.side_block(r, true, old_pics, k, cx)), hatch()),
                _ => (
                    cell(ctx.side_block(r, true, old_pics, k, cx)),
                    cell(ctx.side_block(r, false, new_pics, k, cx)),
                ),
            };
            (
                div()
                    .flex()
                    .gap(px(56.))
                    .child(left)
                    .child(right)
                    .into_any_element(),
                r.change != Change::Same,
            )
        }))
        .collect()
}

/// What every block of one page needs.
struct BlockCtx {
    ix: usize,
    /// A block's text can be selected and copied, as a comment's.
    sel: Selecting,
    on: OnPage,
    notes: Rc<Vec<LineNote>>,
    /// What goes under a block (the composer, its opened comments), by side and first line.
    under: RefCell<HashMap<(bool, u32), Vec<AnyElement>>>,
}

impl BlockCtx {
    /// A block of Unified: the old one when `old` (removed: faded and struck), else the new one, or for a changed
    /// block the two read together.
    fn block(
        &self,
        r: &Row,
        old: bool,
        pics: &Rc<DocPictures>,
        k: usize,
        unified: bool,
        cx: &App,
    ) -> AnyElement {
        let (block, lines) = match (old, r) {
            (
                true,
                Row {
                    old: Some((b, l)), ..
                },
            ) => (b, *l),
            (
                false,
                Row {
                    merged: Some(m),
                    new: Some((_, l)),
                    ..
                },
            ) if unified => (m, *l),
            (
                _,
                Row {
                    new: Some((b, l)), ..
                },
            ) => (b, *l),
            (
                _,
                Row {
                    old: Some((b, l)), ..
                },
            ) => (b, *l),
            _ => return div().into_any_element(),
        };
        let struck = unified && r.change == Change::Removed;
        self.frame(
            doc_block(
                block,
                format!("page-{}-{k}-{old}", self.ix),
                pics.clone(),
                self.sel.clone(),
                cx,
            ),
            bar(r.change),
            struck,
            old,
            lines,
            k,
        )
    }

    /// A block of Split: one side's own (a changed one with its side's words marked).
    fn side_block(
        &self,
        r: &Row,
        old: bool,
        pics: &Rc<DocPictures>,
        k: usize,
        cx: &App,
    ) -> AnyElement {
        self.block(r, old, pics, k, false, cx)
    }

    /// A block in its margin: the change's bar left, the comments on its lines and a comment button right (on
    /// hover).
    fn frame(
        &self,
        body: AnyElement,
        color: Option<Rgba>,
        struck: bool,
        old: bool,
        (first, last): (u32, u32),
        k: usize,
    ) -> AnyElement {
        let count: usize = self
            .notes
            .iter()
            .filter(|n| n.old == old && (first..=last).contains(&n.line))
            .map(|n| n.count)
            .sum();
        let (ix, show, write) = (self.ix, self.on.clone(), self.on.clone());
        let under = self
            .under
            .borrow_mut()
            .remove(&(old, first))
            .unwrap_or_default();
        let group_name = "page-block";
        div()
            .id(("page-block", k * 2 + old as usize))
            .group(group_name)
            .relative()
            .children(color.map(|c| {
                div()
                    .absolute()
                    .left(px(-20.))
                    .top(px(2.))
                    .bottom(px(2.))
                    .w(px(3.))
                    .rounded(px(2.))
                    .bg(c)
            }))
            .child(
                div()
                    .pr(px(64.))
                    .when(struck, |s| s.opacity(0.6).line_through())
                    .child(body),
            )
            .children(under)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .flex()
                    .gap(px(4.))
                    .children((count > 0).then(|| {
                        div()
                            .id(("page-notes", k * 2 + old as usize))
                            .px(px(6.))
                            .h(px(20.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded_full()
                            .bg(theme::info_bg())
                            .text_size(px(11.))
                            .text_color(theme::accent())
                            .cursor_pointer()
                            .child(
                                icons::icon("message")
                                    .size(px(12.))
                                    .text_color(theme::accent()),
                            )
                            .child(count.to_string())
                            .on_click(move |e: &ClickEvent, _, cx| {
                                if e.click_count() > 1 {
                                    return;
                                }
                                show(PageEvent::Notes { ix, old, first }, cx);
                            })
                    }))
                    .child(
                        div()
                            .id(("page-comment", k * 2 + old as usize))
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .invisible()
                            .group_hover(group_name, |s| s.visible())
                            .bg(theme::hover())
                            .text_color(theme::muted())
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::text()))
                            .child(icons::icon("plus").size(px(14.)))
                            .on_click(move |_: &ClickEvent, _, cx| {
                                write(PageEvent::Comment { ix, old, first }, cx)
                            }),
                    ),
            )
            .into_any_element()
    }
}

/// A page's type: the document reads in the interface's font, a size up.
fn page_text<E: Styled>(el: E) -> E {
    el.font_family(theme::ui_font())
        .text_size(px(14.))
        .line_height(px(22.))
        .text_color(theme::text())
}

/// "1 section removed: Known issues", and a click shows it where it was.
fn fold_line(names: &[String], open: bool, key: (String, u32), on: OnPage, k: usize) -> AnyElement {
    let what = match names.len() {
        1 => "1 section removed".to_owned(),
        n => format!("{n} sections removed"),
    };
    div()
        .id(("page-fold", k))
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(10.))
        .h(px(28.))
        .rounded(px(4.))
        .bg(theme::removed_line())
        .text_size(px(12.))
        .text_color(theme::removed())
        .cursor_pointer()
        .child(icons::icon("x").size(px(13.)).text_color(theme::removed()))
        .child(div().flex_none().child(format!("{what}:")))
        .child(
            hover::cut(
                gpui::SharedString::from(format!("{what}-names")),
                names.join(", "),
            )
            .flex_1()
            .font_weight(FontWeight::SEMIBOLD),
        )
        .child(
            div()
                .text_color(theme::faint())
                .child(if open { "hide" } else { "show" }),
        )
        .on_click(move |_: &ClickEvent, _, cx| on(PageEvent::Fold(key.0.clone(), key.1), cx))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn documents_read_as_source_are_kept_per_repository() {
        let text = "a\tREADME.md\nb\tdocs/x.md\n";
        assert_eq!(
            source_paths(text, "a"),
            HashSet::from(["README.md".to_owned()])
        );
        let paths = HashSet::from(["CHANGELOG.md".to_owned()]);
        let saved = with_source_paths(text, "a", &paths);
        assert_eq!(saved, "a\tCHANGELOG.md\nb\tdocs/x.md\n");
        assert_eq!(
            source_paths(&saved, "b"),
            HashSet::from(["docs/x.md".to_owned()])
        );
    }
}
