//! Search in the repository (⌥⌘F, #206): what the IDE calls Find in Files, over every file of the working tree git
//! knows, as a tool window on the right (`tool`; Design `design/explore/audit/code.js`, Search in …): its field with
//! Aa W .*, what it found (`LoginState · 12 in 3 files`, the working tree), the lines grouped by file, each group
//! folding at its head, each line in its colours with the query marked. ↑ ↓ walk the lines, ↵ or a click opens one:
//! the tab follows `ReviewEvent::InFiles`, in the diff on screen when it changes that file, else in Code at its line,
//! and the window stays. Closed, it keeps its words, its toggles and what it found, and lets go of what it read.

use crate::repo_search::{self, Cache, InFile, Look, Narrow, Scan};
use crate::review::{Review, ReviewEvent};
use artoo_core::highlight::{self, Span};
use artoo_core::rows;
use artoo_review_api::{FoundInFiles as Found, SEARCH};
use artoo_shell_api::{Frame, Tool};
use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::input::{self, Area, Edit};
use artoo_ui_kit::{ROW_RADIUS, icons, px, row, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontStyle, FontWeight, HighlightStyle, Keystroke,
    MouseButton, ScrollHandle, StyledText, Task, WeakEntity, Window, div, prelude::*,
};
use std::cell::Cell;
use std::collections::{HashSet, VecDeque};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// No more lines than a person can read.
const MAX_HITS: usize = 400;
/// How long a query waits after the last key before it is searched.
const SETTLE: Duration = Duration::from_millis(120);
/// The files one background job searches before the window hears of them.
const CHUNK: usize = 256;
/// How far into a line its first hit may end, in chars, before the line is shown from nearer it (the design's 36).
const HIT_IN: usize = 36;

/// A line found.
struct Hit {
    file: usize,
    line: u32,
    text: String,
    /// Where the query is in `text`: every place on the line, the first first.
    marks: Vec<Range<usize>>,
    /// Its colours, the line read alone.
    spans: Vec<Span>,
}

pub struct FindFiles {
    query: String,
    /// Its field: the caret, the selection, the sideways scroll.
    area: Area,
    /// The line picked, among `hits`.
    at: usize,
    case: bool,
    word: bool,
    /// The query is a regular expression (`.*`, ⌥R, #113); what is wrong with it, when it is.
    regex: bool,
    error: Option<String>,
    hits: Vec<Hit>,
    /// The repository's files as git's index lists them: paths only, each read when a query needs it.
    paths: Arc<Vec<String>>,
    /// The paths are being listed.
    listing: bool,
    root: PathBuf,
    files_with_hits: usize,
    capped: bool,
    /// The list or the scan under way: dropped, it stops.
    task: Option<Task<()>>,
    /// A scan is under way: how many of its files it has looked at, of how many.
    searching: Option<(usize, usize)>,
    /// The hits on screen are the last query's until the new one's first come in.
    stale: bool,
    /// The last plain query searched to its end, and the files that hold it: a longer one looks only there.
    narrow: Option<Narrow>,
    /// What was read, kept for the next query while the window is open.
    cache: Arc<Mutex<Cache>>,
    scroll: ScrollHandle,
    /// The files whose lines are folded under their head.
    folded: HashSet<usize>,
}

/// What a key typed in the search came to: the window closes on esc.
pub(crate) enum Keyed {
    Kept,
    Close,
}

/// The search as a tool window in a tab: on the stripe, first, in every space; it takes typed keys while it has the
/// keyboard, and its title names the repository.
pub(crate) fn tool(review: Entity<Review>, frame: Rc<dyn Frame>, cx: &mut App) -> Tool {
    let view = cx.new(|_| SearchView {
        review: review.clone(),
        frame: frame.clone(),
    });
    let (named, opened, closed, keyed) = (
        review.downgrade(),
        review.downgrade(),
        review.downgrade(),
        review.downgrade(),
    );
    Tool {
        id: SEARCH,
        icon: "search",
        tip: "Search in the repository  ⌥⌘F",
        title: "Search",
        heading: Some(Rc::new(move |cx| {
            let rv = named.upgrade()?;
            let name = rv.read(cx).root.as_ref()?.file_name()?.to_owned();
            Some(format!("Search in {}", name.to_string_lossy()))
        })),
        spaces: &[],
        on_pages: false,
        stripe: true,
        // The stripe's first, as the design draws it.
        order: 5,
        badge: None,
        count: None,
        view: view.into(),
        opened: Rc::new(move |cx| {
            opened.update(cx, |rv, cx| rv.open_find_files(cx)).ok();
        }),
        closed: Rc::new(move |cx| {
            closed.update(cx, |rv, _| rv.rest_find_files()).ok();
        }),
        typing: Rc::new(Cell::new(true)),
        key: Rc::new(move |event, _, cx| {
            let keyed = keyed.update(cx, |rv, cx| rv.find_files_key(&event.keystroke, cx));
            if let Ok(Keyed::Close) = keyed {
                frame.show_tool(None, cx);
            }
        }),
        list_key: None,
    }
}

/// The window's body: the review's search, drawn.
struct SearchView {
    review: Entity<Review>,
    frame: Rc<dyn Frame>,
}

impl Render for SearchView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.frame.tool_typing(SEARCH, cx);
        let this = self.review.downgrade();
        (self.review.read(cx))
            .render_find_files(active, this, self.frame.clone())
            .unwrap_or_else(|| div().into_any_element())
    }
}

impl FindFiles {
    /// A chunk's files in (`looked` of them looked at): the first replace the last query's hits, the hits stop at
    /// `MAX_HITS`.
    fn take_found(&mut self, found: Vec<(usize, InFile)>, looked: usize) {
        if self.stale && found.iter().any(|(_, f)| f.hit) {
            self.clear_hits();
        }
        if let Some((done, _)) = self.searching.as_mut() {
            *done += looked;
        }
        for (file, in_file) in found {
            if !in_file.hit {
                continue;
            }
            self.files_with_hits += 1;
            if in_file.lines.is_empty() {
                self.capped = true;
            }
            for line in in_file.lines {
                if self.hits.len() >= MAX_HITS {
                    self.capped = true;
                    break;
                }
                let mut marks = vec![line.range];
                marks.extend(line.more);
                self.hits.push(Hit {
                    file,
                    line: line.line,
                    text: line.text,
                    marks,
                    spans: line.spans,
                });
            }
        }
    }

    fn clear_hits(&mut self) {
        self.hits.clear();
        self.files_with_hits = 0;
        self.capped = false;
        self.at = 0;
        self.stale = false;
    }

    /// The hits not folded away, by their index.
    fn shown(&self) -> Vec<usize> {
        (0..self.hits.len())
            .filter(|&ix| !self.folded.contains(&self.hits[ix].file))
            .collect()
    }

    /// The row hit `at` is in the list: the heads before it and the lines shown.
    fn row_of(&self, at: usize) -> usize {
        let mut row = 0;
        let mut last = usize::MAX;
        for (ix, hit) in self.hits.iter().enumerate() {
            if hit.file != last {
                last = hit.file;
                row += 1;
            }
            if ix == at {
                return row;
            }
            if !self.folded.contains(&hit.file) {
                row += 1;
            }
        }
        row
    }

    /// Hit `at` as the tab opens it.
    fn found(&self, at: usize) -> Option<Found> {
        let hit = self.hits.get(at)?;
        Some(Found {
            query: self.query.clone(),
            path: self.paths.get(hit.file)?.clone(),
            line: hit.line,
            old: false,
        })
    }
}

impl Review {
    /// Opens the search on the diff's find's words, else on its own last ones: the repository's paths are listed in
    /// the background, the lines found before shown until the new ones come.
    pub fn open_find_files(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let kept = self.ffind.take().filter(|f| f.root == root);
        let query = (self.dfind.clone().filter(|q| !q.is_empty()))
            .or_else(|| kept.as_ref().map(|f| f.query.clone()))
            .unwrap_or_default();
        let listing = {
            let root = root.clone();
            cx.spawn(async move |this, cx| {
                let paths = cx
                    .background_executor()
                    .spawn(async move { repo_search::repository_paths(&root) })
                    .await;
                this.update(cx, |rv, cx| {
                    if let Some(f) = rv.ffind.as_mut() {
                        // The lines shown name files by their place in the list: another list, they go.
                        if *f.paths != paths {
                            f.clear_hits();
                            f.folded.clear();
                        }
                        f.paths = Arc::new(paths);
                        f.listing = false;
                        f.task = None;
                    }
                    rv.refresh_find_files(cx);
                    cx.notify();
                })
                .ok();
            })
        };
        let mut area = Area::new(query.clone());
        area.edit.select_all();
        let mut f = kept.unwrap_or_else(|| FindFiles {
            query: String::new(),
            area: Area::default(),
            at: 0,
            case: false,
            word: false,
            regex: false,
            error: None,
            hits: Vec::new(),
            paths: Arc::new(Vec::new()),
            listing: true,
            root,
            files_with_hits: 0,
            capped: false,
            task: None,
            searching: None,
            stale: false,
            narrow: None,
            cache: Arc::default(),
            scroll: ScrollHandle::new(),
            folded: HashSet::new(),
        });
        if f.query != query {
            f.narrow = None;
        }
        f.query = query;
        f.area = area;
        f.listing = true;
        f.stale = true;
        f.task = Some(listing);
        self.ffind = Some(f);
    }

    /// The window closed: what it found stays for the next time, what it read and the scan under way go.
    pub fn rest_find_files(&mut self) {
        if let Some(f) = self.ffind.as_mut() {
            f.task = None;
            f.searching = None;
            f.listing = false;
            f.narrow = None;
            f.cache = Arc::default();
        }
    }

    /// The query searched again, in the background, once it has settled: the hits on screen stay until the new
    /// ones come; a plain query that holds the last one's text looks only where that one was (and in files changed
    /// since).
    fn refresh_find_files(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.ffind.as_mut() else {
            return;
        };
        if f.listing {
            return;
        }
        f.task = None;
        f.searching = None;
        f.error = None;
        if f.query.is_empty() {
            f.clear_hits();
            return;
        }
        let query = match repo_search::compile(&f.query, f.case, f.word, f.regex) {
            Ok(query) => query,
            Err(error) => {
                f.error = Some(error);
                f.clear_hits();
                return;
            }
        };
        let narrow = (f.narrow.as_ref())
            .filter(|n| query.plain && n.applies(&f.query, f.case))
            .map(|n| (n.files.iter().copied().collect::<HashSet<_>>(), n.since));
        let looks: Vec<Look> = (0..f.paths.len())
            .map(|file| Look {
                file,
                always: narrow
                    .as_ref()
                    .is_none_or(|(files, _)| files.contains(&file)),
            })
            .collect();
        let plain = query.plain.then(|| (f.query.clone(), f.case));
        let scan = Arc::new(Scan {
            root: f.root.clone(),
            paths: f.paths.clone(),
            query,
            cache: f.cache.clone(),
            full: AtomicBool::new(false),
            since: narrow.map(|(_, since)| since),
        });
        let started = SystemTime::now();
        let total = looks.len();
        f.stale = true;
        f.searching = Some((0, total));
        f.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            let lanes = cx.background_executor().num_cpus().clamp(1, 8);
            let mut chunks = looks.chunks(CHUNK).map(<[Look]>::to_vec);
            let mut pending = VecDeque::new();
            let mut holding = Vec::new();
            loop {
                while pending.len() < lanes {
                    let Some(chunk) = chunks.next() else {
                        break;
                    };
                    let scan = scan.clone();
                    let len = chunk.len();
                    pending.push_back((
                        len,
                        cx.background_executor().spawn(async move {
                            let mut found = repo_search::scan(&scan, &chunk, MAX_HITS);
                            colour(&scan.paths, &mut found);
                            found
                        }),
                    ));
                }
                let Some((len, job)) = pending.pop_front() else {
                    break;
                };
                let found = job.await;
                holding.extend(found.iter().map(|(file, _)| *file));
                let full = &scan.full;
                let alive = this
                    .update(cx, |rv, cx| {
                        let Some(f) = rv.ffind.as_mut() else {
                            return false;
                        };
                        f.take_found(found, len);
                        if f.hits.len() >= MAX_HITS {
                            full.store(true, Ordering::Relaxed);
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !alive {
                    return;
                }
            }
            this.update(cx, |rv, cx| {
                if let Some(f) = rv.ffind.as_mut() {
                    if f.stale {
                        f.clear_hits();
                    }
                    f.searching = None;
                    f.task = None;
                    f.narrow = plain.map(|(query, case)| Narrow {
                        query,
                        case,
                        files: holding,
                        since: started,
                    });
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// A toggle flipped: Match case, Whole word, Regular expression.
    fn find_files_toggle(&mut self, toggle: Toggle, cx: &mut Context<Self>) {
        let Some(f) = self.ffind.as_mut() else {
            return;
        };
        match toggle {
            Toggle::Case => f.case = !f.case,
            Toggle::Word => f.word = !f.word,
            Toggle::Regex => f.regex = !f.regex,
        }
        f.at = 0;
        self.refresh_find_files(cx);
        cx.notify();
    }

    /// A key typed in the search: ⌥C ⌥W ⌥R flip its toggles, ↑ ↓ walk the lines, ↵ opens the one picked, esc
    /// closes the window; the rest edit its field.
    fn find_files_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> Keyed {
        let Some(f) = self.ffind.as_mut() else {
            return Keyed::Kept;
        };
        if key.modifiers.alt && matches!(key.key.as_str(), "c" | "w" | "r") {
            let toggle = match key.key.as_str() {
                "c" => Toggle::Case,
                "w" => Toggle::Word,
                _ => Toggle::Regex,
            };
            self.find_files_toggle(toggle, cx);
            return Keyed::Kept;
        }
        if matches!(key.key.as_str(), "up" | "down") && !key.modifiers.modified() {
            self.find_files_step(key.key == "down");
            cx.notify();
            return Keyed::Kept;
        }
        match input::edit(&mut f.query, &mut f.area.edit, key, cx) {
            Edit::Changed => {
                f.at = 0;
                self.refresh_find_files(cx);
            }
            Edit::Enter { .. } => {
                let at = f.at;
                self.open_hit(at, cx);
            }
            Edit::Escape => return Keyed::Close,
            Edit::Selected | Edit::Ignored => {}
        }
        cx.notify();
        Keyed::Kept
    }

    /// ↑ ↓: the line before or after among those not folded, scrolled to.
    fn find_files_step(&mut self, down: bool) {
        let Some(f) = self.ffind.as_mut() else {
            return;
        };
        let shown = f.shown();
        if shown.is_empty() {
            return;
        }
        let n = shown.len();
        let here = shown.iter().position(|&ix| ix == f.at);
        let to = match (here, down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        f.at = shown[to];
        let row = f.row_of(f.at);
        f.scroll.scroll_to_item(row);
    }

    /// Line `at` picked and opened: the tab shows it, the window stays.
    fn open_hit(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(f) = self.ffind.as_mut() else {
            return;
        };
        let Some(found) = f.found(at) else {
            return;
        };
        f.at = at;
        cx.emit(ReviewEvent::InFiles(found));
        cx.notify();
    }

    /// File `file`'s lines folded under their head, or shown again.
    fn fold_found(&mut self, file: usize, cx: &mut Context<Self>) {
        if let Some(f) = self.ffind.as_mut()
            && !f.folded.remove(&file)
        {
            f.folded.insert(file);
        }
        cx.notify();
    }

    /// The search in its window: its field (`active` with the keyboard), what it found, the lines by file.
    fn render_find_files(
        &self,
        active: bool,
        this: WeakEntity<Review>,
        frame: Rc<dyn Frame>,
    ) -> Option<AnyElement> {
        let f = self.ffind.as_ref()?;
        let act = |doing: fn(&mut Review, &mut Context<Review>)| {
            let this = this.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, doing).ok();
            }
        };
        let placeholder = if f.listing || f.paths.is_empty() {
            "Search".to_owned()
        } else {
            format!("Search {} files", f.paths.len())
        };
        let field = f.area.field_of(
            &f.query,
            active,
            &placeholder,
            false,
            |rv: &mut Review| rv.ffind.as_mut().map(|f| &mut f.area),
            this.clone(),
        );
        let take = frame.clone();
        let search = div().flex_none().px(px(8.)).pt(px(8.)).pb(px(6.)).child(
            div()
                .id("ffind-box")
                .h(px(28.))
                .pl(px(8.))
                .pr(px(4.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(ROW_RADIUS))
                .border_1()
                .border_color(if active {
                    theme::focus()
                } else {
                    theme::island_border()
                })
                .bg(theme::base())
                .cursor_text()
                // A press in it gives the window the keyboard.
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    take.show_tool(Some(SEARCH), cx)
                })
                .child(
                    icons::icon("search")
                        .size(px(14.))
                        .text_color(theme::faint()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .font_family(theme::code_font())
                        .text_size(px(12.))
                        .child(field),
                )
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .gap(px(2.))
                        .child(
                            option("ffind-case", "Aa", f.case, "Match case  ⌥C")
                                .on_click(act(|rv, cx| rv.find_files_toggle(Toggle::Case, cx))),
                        )
                        .child(
                            option("ffind-word", "W", f.word, "Whole word  ⌥W")
                                .on_click(act(|rv, cx| rv.find_files_toggle(Toggle::Word, cx))),
                        )
                        .child(
                            option("ffind-regex", ".*", f.regex, "Regular expression  ⌥R")
                                .on_click(act(|rv, cx| rv.find_files_toggle(Toggle::Regex, cx))),
                        ),
                ),
        );
        Some(
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(search)
                .child(summary(f))
                .child(
                    div()
                        .id("ffind-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&f.scroll)
                        .px(px(6.))
                        .pt(px(6.))
                        .pb(px(12.))
                        .children(rows_of(f, this)),
                )
                .into_any_element(),
        )
    }
}

#[derive(Clone, Copy)]
enum Toggle {
    Case,
    Word,
    Regex,
}

/// One of the field's toggles: `Aa`, `W`, `.*`.
fn option(
    id: &'static str,
    label: &'static str,
    on: bool,
    tip: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .min_w(px(22.))
        .h(px(18.))
        .px(px(4.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(ROW_RADIUS))
        .font_family(theme::code_font())
        .text_size(px(11.))
        .cursor_pointer()
        .when(on, |s| s.bg(theme::selected()).text_color(theme::text()))
        .when(!on, |s| {
            s.text_color(theme::faint())
                .hover(|s| s.bg(theme::hover()).text_color(theme::text()))
        })
        .tooltip_show_delay(hover::LONG)
        .tooltip(move |_, cx| cx.new(|_| Tip(tip)).into())
        .child(label)
}

/// What it found, over the lines: `LoginState · 12 in 3 files`, and where it looked, the working tree.
fn summary(f: &FindFiles) -> impl IntoElement + use<> {
    let matches: usize = f.hits.iter().map(|h| h.marks.len()).sum();
    let left = if let Some(error) = &f.error {
        div()
            .text_color(theme::removed())
            .child(format!("Not a regular expression: {error}"))
            .into_any_element()
    } else if f.listing && f.hits.is_empty() {
        div().child("Listing the files…").into_any_element()
    } else if f.query.is_empty() {
        div().into_any_element()
    } else if (f.hits.is_empty() || f.stale)
        && let Some((done, total)) = f.searching
    {
        div()
            .child(format!("Searching… {done} of {total} files"))
            .into_any_element()
    } else {
        let files = f.files_with_hits;
        div()
            .min_w_0()
            .flex()
            .child(
                hover::cut("ffind-query", f.query.clone())
                    .flex_shrink(1.)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text()),
            )
            .child(div().flex_none().whitespace_nowrap().child(format!(
                " · {}{matches} in {files} file{}{}",
                if f.capped { "first " } else { "" },
                if files == 1 { "" } else { "s" },
                if f.searching.is_some() { " so far" } else { "" },
            )))
            .into_any_element()
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(14.))
        .pt(px(2.))
        .pb(px(8.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .border_b_1()
        .border_color(theme::island_border())
        .child(div().flex_1().min_w_0().overflow_hidden().child(left))
        .child(
            div()
                .flex_none()
                .text_color(theme::faint())
                .child("working tree"),
        )
}

/// The lines found under their files' heads: a head folds its lines.
fn rows_of(f: &FindFiles, this: WeakEntity<Review>) -> Vec<AnyElement> {
    let mut rows = Vec::new();
    let mut ix = 0;
    while ix < f.hits.len() {
        let file = f.hits[ix].file;
        let end = (ix..f.hits.len())
            .find(|&i| f.hits[i].file != file)
            .unwrap_or(f.hits.len());
        let count: usize = f.hits[ix..end].iter().map(|h| h.marks.len()).sum();
        let folded = f.folded.contains(&file);
        let path = f.paths.get(file).cloned().unwrap_or_default();
        let (folder, name) = match path.rfind('/') {
            Some(at) => (path[..at].to_owned(), path[at + 1..].to_owned()),
            None => (String::new(), path.clone()),
        };
        let fold = this.clone();
        rows.push(
            div()
                .id(("ffind-head", file))
                .h(px(28.))
                .mt(px(4.))
                .pl(px(4.))
                .pr(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(ROW_RADIUS))
                .text_size(px(12.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .on_click(move |_, _, cx| {
                    fold.update(cx, |rv, cx| rv.fold_found(file, cx)).ok();
                })
                .child(
                    icons::icon(if folded { "chevron-right" } else { "chevron" })
                        .size(px(13.))
                        .text_color(theme::faint()),
                )
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .child(name),
                )
                .child(
                    hover::cut(("ffind-folder", file), folder)
                        .flex_shrink(1.)
                        .min_w_0()
                        .text_size(px(11.))
                        .text_color(theme::faint()),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_color(theme::faint())
                        .child(count.to_string()),
                )
                .into_any_element(),
        );
        if !folded {
            for at in ix..end {
                rows.push(hit_row(f, at, this.clone()));
            }
        }
        ix = end;
    }
    rows
}

/// Line `at`: its number, and its text from its indent, from nearer its first hit when that is far in.
fn hit_row(f: &FindFiles, at: usize, this: WeakEntity<Review>) -> AnyElement {
    let hit = &f.hits[at];
    let (text, marks, spans) = shown_line(hit);
    let runs = rows::runs(&text, &spans, &[], &marks, None);
    let styled = StyledText::new(text.clone()).with_highlights(runs.into_iter().map(|run| {
        let style = if run.found {
            HighlightStyle {
                color: Some(theme::text().into()),
                background_color: Some(theme::warning_bg().into()),
                ..Default::default()
            }
        } else {
            HighlightStyle {
                color: run.color.map(theme::code),
                ..Default::default()
            }
        };
        (
            run.range,
            HighlightStyle {
                font_style: run.italic.then_some(FontStyle::Italic),
                ..style
            },
        )
    }));
    row(("ffind-hit", at), at == f.at)
        .min_h(px(24.))
        .py(px(2.))
        .pl(px(20.))
        .pr(px(8.))
        .gap(px(8.))
        .child(
            div()
                .flex_none()
                .min_w(px(22.))
                .text_align(gpui::TextAlign::Right)
                .font_family(theme::code_font())
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(hit.line.to_string()),
        )
        .child(
            hover::cut_with("ffind-line", text, styled)
                .flex_1()
                .font_family(theme::code_font())
                .text_size(px(12.))
                .text_color(theme::muted()),
        )
        .on_click(move |_, _, cx| {
            this.update(cx, |rv, cx| rv.open_hit(at, cx)).ok();
        })
        .into_any_element()
}

/// A hit's line as its row shows it: without its indent, and from `…` nearer the first hit when that ends past
/// `HIT_IN` chars; its marks and colours moved with it.
fn shown_line(hit: &Hit) -> (String, Vec<Range<usize>>, Vec<Span>) {
    let text = &hit.text;
    let indent = text.len() - text.trim_start().len();
    let first_end = hit.marks.first().map_or(indent, |m| m.end.max(indent));
    let chars_in = text[indent..first_end].chars().count();
    let skip = if chars_in > HIT_IN {
        (text[indent..].char_indices())
            .nth(chars_in - (HIT_IN - 1))
            .map_or(0, |(at, _)| at)
    } else {
        0
    };
    let from = indent + skip;
    let lead = if skip > 0 { "…" } else { "" };
    let moved = |r: &Range<usize>| {
        (r.end > from).then(|| r.start.max(from) - from + lead.len()..r.end - from + lead.len())
    };
    let marks = hit.marks.iter().filter_map(moved).collect();
    let spans = (hit.spans.iter())
        .filter_map(|s| moved(&s.range).map(|range| Span { range, ..s.clone() }))
        .collect();
    (format!("{lead}{}", &text[from..]), marks, spans)
}

/// The lines a chunk found, coloured as their files' language reads them, each line alone: no more than a list
/// shows.
fn colour(paths: &[String], found: &mut [(usize, InFile)]) {
    let mut room = MAX_HITS;
    for (file, in_file) in found.iter_mut() {
        let Some(path) = paths.get(*file) else {
            continue;
        };
        for line in &mut in_file.lines {
            if room == 0 {
                return;
            }
            room -= 1;
            line.spans = (highlight::highlight(path, &line.text).into_iter())
                .next()
                .unwrap_or_default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(text: &str, first: Range<usize>) -> Hit {
        Hit {
            file: 0,
            line: 1,
            text: text.to_owned(),
            marks: vec![first],
            spans: Vec::new(),
        }
    }

    #[test]
    fn a_line_is_shown_from_its_indent_and_from_nearer_a_far_hit() {
        let (text, marks, _) = shown_line(&hit("    let x = LoginState;", 12..22));
        assert_eq!(text, "let x = LoginState;");
        assert_eq!(&text[marks[0].clone()], "LoginState");
        let far = format!("  {}LoginState", "a".repeat(50));
        let (text, marks, _) = shown_line(&hit(&far, 52..62));
        assert!(text.starts_with('…'));
        assert_eq!(&text[marks[0].clone()], "LoginState");
        assert_eq!(text.chars().count(), 1 + HIT_IN - 1);
    }
}
