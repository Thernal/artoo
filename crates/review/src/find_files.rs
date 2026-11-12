//! Search in the repository (⌘⌥F, #206): what the IDE calls Find in Files, over every file of the working tree
//! git knows. Results are grouped by file; ↵ opens one and leaves the match marked in the file.
//! Designed in `../Design/mockups/ide-ideas/a-ideas.html` (3). The tab draws it over the window and
//! answers its clicks (`SearchEvent`); its field is the tab's.

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
use std::ops::Range;

/// No more results than a person can read.
const MAX_HITS: usize = 400;

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
    /// The repository's text files, read as it opened: (path, lines).
    files: std::rc::Rc<Vec<(String, Vec<String>)>>,
    /// The repository's name, the window's title.
    name: String,
    files_with_hits: usize,
    capped: bool,
    scroll: ScrollHandle,
}

pub use artoo_review_api::{FoundInFiles as Found, OnSearch, SearchEvent, SearchKeyed as Keyed};

/// Where `needle` first occurs in `hay` (a byte range), under the toggles.
fn find_in(hay: &str, needle: &str, case: bool, word: bool) -> Option<Range<usize>> {
    if needle.is_empty() {
        return None;
    }
    let lowered;
    let (hay_cmp, needle_cmp): (&str, String) = if case || !hay.is_ascii() {
        (hay, needle.to_owned())
    } else {
        lowered = hay.to_ascii_lowercase();
        (lowered.as_str(), needle.to_ascii_lowercase())
    };
    let mut from = 0;
    while let Some(at) = hay_cmp[from..].find(&needle_cmp) {
        let start = from + at;
        let end = start + needle_cmp.len();
        let boundary = |c: Option<char>| c.is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        if !word
            || (boundary(hay[..start].chars().next_back()) && boundary(hay[end..].chars().next()))
        {
            return Some(start..end);
        }
        from = end;
    }
    None
}

/// No more than this many files, none longer than this, are searched: what a person waits for.
const MAX_FILES: usize = 20_000;
const MAX_BYTES: u64 = 1 << 20;

/// The repository's text files as git knows them, read from the working tree.
pub(crate) fn repository_files(root: &std::path::Path) -> Vec<(String, Vec<String>)> {
    let Ok(repo) = git2::Repository::open(root) else {
        return Vec::new();
    };
    let Ok(index) = repo.index() else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in index.iter().take(MAX_FILES) {
        let path = String::from_utf8_lossy(&entry.path).into_owned();
        let full = root.join(&path);
        if std::fs::metadata(&full).map_or(true, |m| !m.is_file() || m.len() > MAX_BYTES) {
            continue;
        }
        let Ok(bytes) = std::fs::read(&full) else {
            continue;
        };
        if artoo_core::files::binary(&bytes) {
            continue;
        }
        let lines = String::from_utf8_lossy(&bytes)
            .lines()
            .map(str::to_owned)
            .collect();
        files.push((path, lines));
    }
    files
}

impl Review {
    /// Opens the search, the diff's find's words in it.
    pub fn open_find_files(&mut self) {
        let query = self.dfind.clone().unwrap_or_default();
        let root = self.root.clone();
        let name = (root.as_ref())
            .and_then(|r| r.file_name())
            .map_or("the repository".to_owned(), |n| {
                n.to_string_lossy().into_owned()
            });
        let files = std::rc::Rc::new(root.as_deref().map(repository_files).unwrap_or_default());
        self.ffind = Some(FindFiles {
            files,
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
            scroll: ScrollHandle::new(),
        });
        self.refresh_find_files();
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

    fn refresh_find_files(&mut self) {
        let Some(f) = &self.ffind else {
            return;
        };
        let files = f.files.clone();
        let (query, case, word) = (f.query.clone(), f.case, f.word);
        // As a regular expression: the case and whole-word toggles go into it.
        let pattern = (f.regex && !query.is_empty()).then(|| {
            let source = if word {
                format!(r"\b(?:{query})\b")
            } else {
                query.clone()
            };
            regex::RegexBuilder::new(&source)
                .case_insensitive(!case)
                .build()
                .map_err(|e| match e {
                    regex::Error::Syntax(s) => s.lines().last().unwrap_or("").trim().to_owned(),
                    other => other.to_string(),
                })
        });
        let error = pattern.as_ref().and_then(|p| p.as_ref().err().cloned());
        let pattern = pattern.and_then(Result::ok);
        let find = |text: &str| match &pattern {
            Some(re) => re
                .find_iter(text)
                .find(|m| !m.is_empty())
                .map(|m| m.range()),
            None if error.is_some() => None,
            None => find_in(text, &query, case, word),
        };
        let mut hits = Vec::new();
        let mut files_with_hits = 0;
        let mut capped = false;
        if !query.is_empty() {
            'files: for (file_ix, (_, lines)) in files.iter().enumerate() {
                let before = hits.len();
                for (n, text) in lines.iter().enumerate() {
                    if let Some(range) = find(text) {
                        if hits.len() >= MAX_HITS {
                            capped = true;
                            break 'files;
                        }
                        hits.push(Hit {
                            file: file_ix,
                            line: n as u32 + 1,
                            old: false,
                            text: text.clone(),
                            range,
                        });
                    }
                }
                if hits.len() > before {
                    files_with_hits += 1;
                }
            }
        }
        if let Some(f) = &mut self.ffind {
            f.at = f.at.min(hits.len().saturating_sub(1));
            f.error = error;
            f.hits = hits;
            f.files_with_hits = files_with_hits;
            f.capped = capped;
        }
    }

    /// A toggle flipped (Case, Word, Regex); any other event is the tab's.
    pub fn find_files_toggle(&mut self, toggle: SearchEvent) {
        let Some(f) = self.ffind.as_mut() else {
            return;
        };
        match toggle {
            SearchEvent::Case => f.case = !f.case,
            SearchEvent::Word => f.word = !f.word,
            SearchEvent::Regex => f.regex = !f.regex,
            SearchEvent::Close | SearchEvent::Open(_) => return,
        }
        self.refresh_find_files();
    }

    /// A key typed in the search, edited in the tab's `field`.
    pub fn find_files_key(&mut self, key: &Keystroke, field: &mut TextEdit, cx: &mut App) -> Keyed {
        let Some(f) = self.ffind.as_mut() else {
            return Keyed::Kept;
        };
        if key.modifiers.alt && matches!(key.key.as_str(), "c" | "w" | "r") {
            self.find_files_toggle(match key.key.as_str() {
                "c" => SearchEvent::Case,
                "w" => SearchEvent::Word,
                _ => SearchEvent::Regex,
            });
            return Keyed::Kept;
        }
        match input::edit(&mut f.query, field, key, cx) {
            Edit::Changed => {
                f.at = 0;
                self.refresh_find_files();
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
        let path = f.files.get(hit.file).map(|(path, _)| path.clone())?;
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
                        .child(hover::path(("hit-path", ix), f.files[hit.file].0.clone()).flex_1())
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
        let summary = if f.query.is_empty() {
            format!("Type to search {} files.", f.files.len())
        } else if f.hits.is_empty() {
            "Nothing matches.".to_owned()
        } else {
            format!(
                "{} · {}{} in {}",
                f.query,
                if f.capped { "first " } else { "" },
                f.hits.len(),
                plural(f.files_with_hits, "file")
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

#[cfg(test)]
mod tests {
    use super::find_in;

    #[test]
    fn finds_ignoring_case_by_default() {
        assert_eq!(
            find_in("let Backoff = 1;", "backoff", false, false),
            Some(4..11)
        );
        assert_eq!(find_in("let Backoff = 1;", "backoff", true, false), None);
    }

    #[test]
    fn whole_word_skips_longer_words() {
        assert_eq!(
            find_in("backoff_delay backoff", "backoff", false, true),
            Some(14..21)
        );
        assert_eq!(find_in("backoffs", "backoff", false, true), None);
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert_eq!(find_in("anything", "", false, false), None);
    }
}
