//! Search in the repository (⌘⌥F, #206): what the IDE calls Find in Files, over every file of the working tree
//! git knows. Results are grouped by file; ↵ opens one and leaves the match marked in the file.
//! Designed in `../Design/mockups/ide-ideas/a-ideas.html` (3). The tab draws it over the window and
//! answers its clicks (`SearchEvent`); its field is the tab's.

use crate::repo_search::{self, Cache, InFile, Look, Narrow, Scan};
use crate::review::Review;
use artoo_core::textedit::TextEdit;
use artoo_ui_kit::format::plural;
use artoo_ui_kit::hover::{self, Tip};
use artoo_ui_kit::input::{self, Edit};
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, icons, island_label, px, row, theme};
use gpui::{
    AnyElement, App, ClickEvent, FontWeight, HighlightStyle, Keystroke, MouseButton, ScrollHandle,
    StyledText, Window, div, prelude::*,
};
use gpui::{Context, Task};
use std::collections::{HashSet, VecDeque};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// No more results than a person can read.
const MAX_HITS: usize = 400;
/// How long a query waits after the last key before it is searched.
const SETTLE: Duration = Duration::from_millis(120);
/// The files one background job searches before the window hears of them.
const CHUNK: usize = 256;

struct Hit {
    file: usize,
    line: u32,
    old: bool,
    text: String,
    range: Range<usize>,
}

pub struct FindFiles {
    query: String,
    at: usize,
    case: bool,
    word: bool,
    /// The query is a regular expression (`.*`, ⌥R, #113); what is wrong with it, when it is.
    regex: bool,
    error: Option<String>,
    hits: Vec<Hit>,
    /// The repository's files as git's index lists them: paths only, each read when a query needs it.
    paths: Arc<Vec<String>>,
    /// The paths are still being listed.
    listing: bool,
    root: PathBuf,
    /// The repository's name, the window's title.
    name: String,
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
    /// What was read, kept for the next query.
    cache: Arc<Mutex<Cache>>,
    scroll: ScrollHandle,
}

pub use artoo_review_api::{FoundInFiles as Found, OnSearch, SearchEvent, SearchKeyed as Keyed};

impl FindFiles {
    /// A chunk's files in (`looked` of them looked at): the first replace the last query's hits, the hits stop at
    /// `MAX_HITS`.
    fn take_found(&mut self, found: Vec<(usize, InFile)>, looked: usize) {
        if self.stale && found.iter().any(|(_, f)| f.hit) {
            self.hits.clear();
            self.files_with_hits = 0;
            self.capped = false;
            self.at = 0;
            self.stale = false;
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
                self.hits.push(Hit {
                    file,
                    line: line.line,
                    old: false,
                    text: line.text,
                    range: line.range,
                });
            }
        }
    }
}

impl Review {
    /// Opens the search, the diff's find's words in it: the repository's paths are listed in the background, and
    /// nothing is read until there is something to look for.
    pub fn open_find_files(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let query = self.dfind.clone().unwrap_or_default();
        let name = root.file_name().map_or("the repository".to_owned(), |n| {
            n.to_string_lossy().into_owned()
        });
        let listing = {
            let root = root.clone();
            cx.spawn(async move |this, cx| {
                let paths = cx
                    .background_executor()
                    .spawn(async move { repo_search::repository_paths(&root) })
                    .await;
                this.update(cx, |rv, cx| {
                    if let Some(f) = rv.ffind.as_mut() {
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
        self.ffind = Some(FindFiles {
            paths: Arc::new(Vec::new()),
            listing: true,
            root,
            name,
            query,
            at: 0,
            case: false,
            word: false,
            regex: false,
            error: None,
            hits: Vec::new(),
            files_with_hits: 0,
            capped: false,
            task: Some(listing),
            searching: None,
            stale: false,
            narrow: None,
            cache: Arc::default(),
            scroll: ScrollHandle::new(),
        });
    }

    /// Closes the search; whether it was open.
    pub fn close_find_files(&mut self) -> bool {
        self.ffind.take().is_some()
    }

    pub fn find_files_open(&self) -> bool {
        self.ffind.is_some()
    }

    /// What is typed in the search, while it is open.
    pub fn find_files_query(&self) -> Option<&str> {
        self.ffind.as_ref().map(|f| f.query.as_str())
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
            f.hits.clear();
            f.files_with_hits = 0;
            f.capped = false;
            f.stale = false;
            return;
        }
        let query = match repo_search::compile(&f.query, f.case, f.word, f.regex) {
            Ok(query) => query,
            Err(error) => {
                f.error = Some(error);
                f.hits.clear();
                f.files_with_hits = 0;
                f.stale = false;
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
                        cx.background_executor()
                            .spawn(async move { repo_search::scan(&scan, &chunk, MAX_HITS) }),
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
                        f.hits.clear();
                        f.files_with_hits = 0;
                        f.capped = false;
                        f.stale = false;
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

    /// A toggle flipped (Case, Word, Regex); any other event is the tab's.
    pub fn find_files_toggle(&mut self, toggle: SearchEvent, cx: &mut Context<Self>) {
        let Some(f) = self.ffind.as_mut() else {
            return;
        };
        match toggle {
            SearchEvent::Case => f.case = !f.case,
            SearchEvent::Word => f.word = !f.word,
            SearchEvent::Regex => f.regex = !f.regex,
            SearchEvent::Close | SearchEvent::Open(_) => return,
        }
        self.refresh_find_files(cx);
    }

    /// A key typed in the search, edited in the tab's `field`.
    pub fn find_files_key(
        &mut self,
        key: &Keystroke,
        field: &mut TextEdit,
        cx: &mut Context<Self>,
    ) -> Keyed {
        let Some(f) = self.ffind.as_mut() else {
            return Keyed::Kept;
        };
        if key.modifiers.alt && matches!(key.key.as_str(), "c" | "w" | "r") {
            self.find_files_toggle(
                match key.key.as_str() {
                    "c" => SearchEvent::Case,
                    "w" => SearchEvent::Word,
                    _ => SearchEvent::Regex,
                },
                cx,
            );
            return Keyed::Kept;
        }
        match input::edit(&mut f.query, field, key, cx) {
            Edit::Changed => {
                f.at = 0;
                self.refresh_find_files(cx);
            }
            Edit::Enter { .. } => {
                let at = f.at;
                return match self.open_found(at) {
                    Some(found) => Keyed::Open(found),
                    None if self.ffind.is_some() => Keyed::Kept,
                    None => Keyed::Closed,
                };
            }
            Edit::Escape => {
                self.ffind = None;
                return Keyed::Closed;
            }
            Edit::Selected | Edit::Ignored => {}
        }
        Keyed::Kept
    }

    /// ↑ ↓: the hit before or after, scrolled to; whether there was one.
    pub fn find_files_step(&mut self, down: bool) -> bool {
        let Some(f) = self.ffind.as_mut() else {
            return false;
        };
        if f.hits.is_empty() {
            return false;
        }
        let n = f.hits.len();
        f.at = if down {
            (f.at + 1) % n
        } else {
            (f.at + n - 1) % n
        };
        // The list holds a heading per file as well as the hits.
        let row = f.at
            + f.hits[..=f.at]
                .iter()
                .map(|h| h.file)
                .collect::<std::collections::BTreeSet<_>>()
                .len();
        f.scroll.scroll_to_item(row.saturating_sub(1));
        true
    }

    /// Hit `at` opened: the search closes and says where the hit is. One that is not there keeps it open.
    pub fn open_found(&mut self, at: usize) -> Option<Found> {
        let f = self.ffind.take()?;
        let Some(hit) = f.hits.get(at) else {
            self.ffind = Some(f);
            return None;
        };
        let path = f.paths.get(hit.file).cloned()?;
        Some(Found {
            query: f.query.clone(),
            path,
            line: hit.line,
            old: hit.old,
        })
    }

    /// The search over the window, its `field` drawn by the tab; `on` answers its clicks.
    pub fn render_find_files(&self, field: AnyElement, on: OnSearch) -> Option<AnyElement> {
        let f = self.ffind.as_ref()?;
        let click = |event: SearchEvent| {
            let on = on.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| on(event, window, cx)
        };
        let toggle = |id: &'static str, label: &'static str, on: bool, tip: &'static str| {
            div()
                .id(id)
                .px(px(6.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if on { theme::text() } else { theme::muted() })
                .when(on, |s| s.bg(theme::selected()))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .tooltip_show_delay(hover::LONG)
                .tooltip(move |_, cx| cx.new(|_| Tip(tip)).into())
                .child(label)
        };
        let mut rows = Vec::new();
        let mut last_file = usize::MAX;
        for (ix, hit) in f.hits.iter().enumerate() {
            if hit.file != last_file {
                last_file = hit.file;
                let count = f.hits.iter().filter(|h| h.file == hit.file).count();
                rows.push(
                    div()
                        .px(px(14.))
                        .pt(px(8.))
                        .pb(px(2.))
                        .flex()
                        .gap_2()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::faint())
                        .child(hover::path(("hit-path", ix), f.paths[hit.file].clone()).flex_1())
                        .child(count.to_string())
                        .into_any_element(),
                );
            }
            let trimmed = hit.text.trim_start();
            let skip = hit.text.len() - trimmed.len();
            let range = hit.range.start.saturating_sub(skip)..hit.range.end.saturating_sub(skip);
            let styled = StyledText::new(trimmed.to_owned()).with_highlights([(
                range,
                HighlightStyle {
                    color: Some(theme::warning().into()),
                    background_color: Some(theme::warning_bg().into()),
                    ..Default::default()
                },
            )]);
            rows.push(
                row(("ffind-hit", ix), ix == f.at)
                    .h(px(26.))
                    .gap_3()
                    .child(
                        div()
                            .w(px(46.))
                            .flex_none()
                            .text_align(gpui::TextAlign::Right)
                            .font_family(theme::code_font())
                            .text_size(px(11.))
                            .text_color(if hit.old {
                                theme::removed()
                            } else {
                                theme::muted()
                            })
                            .child(format!("{}{}", if hit.old { "−" } else { "" }, hit.line)),
                    )
                    .child(
                        hover::cut_with("hit-text", trimmed.to_owned(), styled)
                            .flex_1()
                            .font_family(theme::code_font())
                            .text_size(px(12.)),
                    )
                    .on_click(click(SearchEvent::Open(ix)))
                    .into_any_element(),
            );
        }
        let summary = if f.listing {
            "Listing the repository's files…".to_owned()
        } else if f.query.is_empty() {
            format!("Type to search {} files.", f.paths.len())
        } else if f.hits.is_empty() || f.stale {
            match f.searching {
                Some((done, total)) => format!("Searching… {done} of {total} files"),
                None => "Nothing matches.".to_owned(),
            }
        } else {
            format!(
                "{} · {}{} in {}{}",
                f.query,
                if f.capped { "first " } else { "" },
                f.hits.len(),
                plural(f.files_with_hits, "file"),
                if f.searching.is_some() { " so far" } else { "" },
            )
        };
        let close = on.clone();
        Some(
            div()
                .id("ffind-backdrop")
                .absolute()
                .size_full()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(72.))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    close(SearchEvent::Close, window, cx)
                })
                .child(
                    div()
                        .occlude()
                        .w(px(720.))
                        .max_h(px(620.))
                        .flex()
                        .flex_col()
                        .rounded(px(ISLAND_RADIUS))
                        .border_1()
                        .border_color(theme::island_border())
                        .bg(theme::panel())
                        .shadow_lg()
                        .overflow_hidden()
                        .child(island_label(format!("Search in {}", f.name)))
                        .child(
                            div()
                                .h(px(40.))
                                .mx(px(10.))
                                .px(px(10.))
                                .mb(px(6.))
                                .flex()
                                .items_center()
                                .gap_2()
                                .rounded(px(ROW_RADIUS))
                                .border_2()
                                .border_color(theme::focus())
                                .bg(theme::base())
                                .child(
                                    icons::icon("search")
                                        .size(px(14.))
                                        .text_color(theme::faint()),
                                )
                                .child(field)
                                .child(
                                    toggle("ffind-case", "Aa", f.case, "Match case  ⌥C")
                                        .on_click(click(SearchEvent::Case)),
                                )
                                .child(
                                    toggle("ffind-word", "W", f.word, "Whole word  ⌥W")
                                        .on_click(click(SearchEvent::Word)),
                                )
                                .child(
                                    toggle("ffind-regex", ".*", f.regex, "Regular expression  ⌥R")
                                        .on_click(click(SearchEvent::Regex)),
                                ),
                        )
                        .child(
                            div()
                                .px(px(14.))
                                .pb(px(4.))
                                .text_size(px(12.))
                                .text_color(if f.error.is_some() {
                                    theme::removed()
                                } else {
                                    theme::muted()
                                })
                                .child(match &f.error {
                                    Some(error) => format!("Not a regular expression: {error}"),
                                    None => summary,
                                }),
                        )
                        .child(
                            div()
                                .id("ffind-list")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .track_scroll(&f.scroll)
                                .px(px(6.))
                                .pb(px(8.))
                                .children(rows),
                        ),
                )
                .into_any_element(),
        )
    }
}
