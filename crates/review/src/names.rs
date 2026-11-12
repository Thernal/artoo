//! Going to a name with no language server, going to a line, and where you are, in Code's file (#207).
//!
//! The repository's declarations by name (`artoo_core::symbols::Names`), read by keyword from its files as the
//! Outline reads one file's, are built in the background at the first ⌘-click or F12 on a name — never at start —
//! and brought up to date at each one after: only the files changed since are read again. Holding ⌘ underlines a
//! declared name under the pointer (the first hold reads the names). The peek opens under the line, scrolled into
//! view, its name underlined in the accent, headed `By name · 3`: the file read's own declaration first, then the
//! others; ↑ ↓ choose, ↵ opens it in Code's preview tab, ⌘↵ (or a ⌘-click) in a tab of its own, ⇧F12 searches the
//! word in the repository, esc closes. A name declared once, elsewhere, is opened at once. ⌃G puts `Line` in the
//! file's head, ↵ goes there, `42:7` to its seventh character with the caret. Each is one of the tab's inputs
//! (`peek_input`, `line_input`).

use crate::review::{Review, ReviewEvent};
use artoo_core::symbols::{self, Declared, Names};
use artoo_shell_api::{Input, Typed};
use artoo_ui_kit::{keys, px, theme};
use gpui::{AnyElement, App, Context, Entity, Keystroke, WeakEntity, div, prelude::*};
use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

/// The peek's place among the tab's inputs.
pub const PEEK_INPUT: &str = artoo_review_api::PEEK_INPUT;
/// The `Line` field's place among the tab's inputs.
pub const LINE_INPUT: &str = artoo_review_api::LINE_INPUT;

/// The declarations of a name, open under its line.
pub struct Peek {
    pub word: String,
    /// The line (0-based) it is under, and the name's bytes in it: underlined while the peek is open.
    pub line: usize,
    pub range: Range<usize>,
    /// Not yet scrolled into view, as it opened or as its declarations came: the list scrolls as far as it reaches
    /// past the window's foot.
    pub reveal: Cell<bool>,
    /// Its declarations, the file read's first; `None` while the names are being read.
    pub found: Option<Vec<Declared>>,
    /// The names were brought up to date for it.
    pub fresh: bool,
    pub sel: usize,
}

/// `names` brought up to the repository at `root` as it is now: a file changed or new read again (by its time and
/// length), one gone forgotten, the rest as they were.
fn up_to_date(root: &std::path::Path, mut names: Names) -> Names {
    let paths: Vec<String> = crate::repo_search::repository_paths(root)
        .into_iter()
        .filter(|p| symbols::is_code(p))
        .collect();
    let now: std::collections::HashSet<&str> = paths.iter().map(String::as_str).collect();
    let gone: Vec<String> = (names.paths())
        .filter(|p| !now.contains(p.as_str()))
        .cloned()
        .collect();
    for path in gone {
        names.forget(&path);
    }
    for path in &paths {
        let full = root.join(path);
        let Ok(meta) = std::fs::metadata(&full) else {
            names.forget(path);
            continue;
        };
        if !meta.is_file() || meta.len() > crate::repo_search::MAX_BYTES {
            names.forget(path);
            continue;
        }
        let stamp = (meta.modified().ok(), meta.len());
        if names.stamp(path) == Some(stamp) {
            continue;
        }
        match std::fs::read(&full) {
            Ok(bytes) if !artoo_core::files::binary(&bytes) => {
                names.put(path, stamp, String::from_utf8_lossy(&bytes).lines());
            }
            _ => names.forget(path),
        }
    }
    names
}

/// What the peek's card draws, taken out of the review for the list's rows.
pub struct PeekLook {
    pub word: String,
    pub found: Option<Vec<Declared>>,
    pub sel: usize,
    /// The file read: its own declaration is marked.
    pub here: String,
}

impl PeekLook {
    pub fn of(peek: &Peek, here: &str) -> Self {
        Self {
            word: peek.word.clone(),
            found: peek.found.clone(),
            sel: peek.sel,
            here: here.to_owned(),
        }
    }
}

impl Review {
    /// ⌘-click or F12 on byte `at` of line `line` (0-based) of the file read, whose text selects as `note`: the
    /// declarations of the name there, under its line. Whether there was a name.
    pub fn peek_name(
        &mut self,
        note: &str,
        line: usize,
        at: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.file_view.as_ref() else {
            return false;
        };
        if note != crate::file_view::selects_as(&view.path) {
            return false;
        }
        let plain = view.plain.borrow();
        let Some((word, range)) = (plain.get(line))
            .and_then(|t| symbols::word_range(t, at).map(|r| (t[r.clone()].to_owned(), r)))
        else {
            return false;
        };
        drop(plain);
        self.file_line = None;
        self.peek = Some(Peek {
            word,
            line,
            range,
            reveal: Cell::new(true),
            found: None,
            fresh: false,
            sel: 0,
        });
        self.fill_peek(cx);
        cx.emit(ReviewEvent::Changed);
        true
    }

    /// The peek's declarations from the names, brought up to date first.
    fn fill_peek(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let here = (self.file_view.as_ref()).map_or(String::new(), |v| v.path.clone());
        let fresh = self.peek.as_ref().is_some_and(|p| p.fresh);
        match &self.names {
            Some((read, names)) if *read == root && fresh => {
                let Some(peek) = self.peek.as_mut() else {
                    return;
                };
                let found = names.of(&peek.word, &here);
                // Declared once, and not on this line: straight there.
                if let [only] = found.as_slice()
                    && !(only.path == here && only.line as usize == peek.line + 1)
                {
                    let (path, line) = (only.path.clone(), only.line);
                    self.peek = None;
                    cx.emit(ReviewEvent::Go {
                        path,
                        line,
                        keep: false,
                    });
                    return;
                }
                peek.found = Some(found);
                peek.reveal.set(true);
            }
            _ => self.read_names(cx),
        }
    }

    /// The repository's names brought up to date in the background, unless they are being read: then the peek
    /// open is filled from them.
    fn read_names(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        if self.names_reading.is_some() {
            return;
        }
        let kept = (self.names.take())
            .filter(|(read, _)| *read == root)
            .map(|(_, names)| Arc::unwrap_or_clone(names))
            .unwrap_or_default();
        self.names_reading = Some(cx.spawn(async move |this, cx| {
            let read = root.clone();
            let names = (cx.background_executor())
                .spawn(async move { up_to_date(&read, kept) })
                .await;
            this.update(cx, |rv, cx| {
                rv.names_reading = None;
                rv.names = Some((root, Arc::new(names)));
                if let Some(peek) = rv.peek.as_mut() {
                    peek.fresh = true;
                    rv.fill_peek(cx);
                }
                cx.emit(ReviewEvent::Changed);
            })
            .ok();
        }));
    }

    /// The pointer over byte `at` of line `line` (0-based) of the file read, `None` off its text; ⌘ held or not.
    /// The first time ⌘ is held over a name, the names are read: only a declared one is underlined.
    pub fn point_name(
        &mut self,
        line: usize,
        at: Option<usize>,
        cmd: bool,
        cx: &mut Context<Self>,
    ) {
        let was = self.underlined();
        let range = self.file_view.as_ref().zip(at).and_then(|(view, at)| {
            let plain = view.plain.borrow();
            let text = plain.get(line)?;
            let on_word =
                (text.get(at..)?.chars().next()).is_some_and(|c| c.is_alphanumeric() || c == '_');
            on_word.then(|| symbols::word_range(text, at)).flatten()
        });
        self.pointed = range.map(|r| (line, r));
        self.cmd = cmd;
        if cmd && self.pointed.is_some() && self.names.is_none() {
            self.read_names(cx);
        }
        if self.underlined() != was {
            cx.notify();
        }
    }

    /// The pointer left line `line`'s text.
    pub fn left_name(&mut self, line: usize, cx: &mut Context<Self>) {
        if self.pointed.as_ref().is_some_and(|(n, _)| *n == line) {
            let was = self.underlined();
            self.pointed = None;
            if was.is_some() {
                cx.notify();
            }
        }
    }

    /// ⌘ went down or up.
    pub fn cmd_held(&mut self, held: bool, cx: &mut Context<Self>) {
        if self.cmd == held {
            return;
        }
        let was = self.underlined();
        self.cmd = held;
        if held && self.pointed.is_some() && self.names.is_none() {
            self.read_names(cx);
        }
        if self.underlined() != was {
            cx.notify();
        }
    }

    /// The name underlined in Code's file: its line and bytes, and whether it is the peek's (in the accent) —
    /// else the one under the pointer while ⌘ is held, when the names say it is declared.
    pub fn underlined(&self) -> Option<(usize, Range<usize>, bool)> {
        if let Some(peek) = &self.peek {
            return Some((peek.line, peek.range.clone(), true));
        }
        if !self.cmd {
            return None;
        }
        let (line, range) = self.pointed.clone()?;
        let (_, names) = self.names.as_ref()?;
        let plain = self.file_view.as_ref()?.plain.borrow();
        let word = plain.get(line)?.get(range.clone())?;
        names.declares(word).then_some((line, range, false))
    }

    /// The peek closes.
    pub fn close_peek(&mut self) -> bool {
        self.peek.take().is_some()
    }

    /// Declaration `ix` of the peek opened: the peek closes, the tab opens it — in a tab of its own when `keep`.
    pub fn open_declared(&mut self, ix: usize, keep: bool, cx: &mut Context<Self>) {
        let Some(peek) = self.peek.take() else {
            return;
        };
        if let Some(d) = peek.found.as_ref().and_then(|f| f.get(ix)) {
            cx.emit(ReviewEvent::Go {
                path: d.path.clone(),
                line: d.line,
                keep,
            });
        }
        cx.emit(ReviewEvent::Changed);
    }

    /// A key while the peek has the keyboard.
    fn peek_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> Typed {
        let Some(peek) = self.peek.as_mut() else {
            return Typed::Leave;
        };
        let n = peek.found.as_ref().map_or(0, Vec::len);
        match key.key.as_str() {
            "up" if n > 0 => peek.sel = (peek.sel + n - 1) % n,
            "down" if n > 0 => peek.sel = (peek.sel + 1) % n,
            "enter" => {
                let at = peek.sel;
                self.open_declared(at, key.modifiers.platform, cx);
                return Typed::Leave;
            }
            "f12" if key.modifiers.shift => {
                let word = peek.word.clone();
                self.peek = None;
                cx.emit(ReviewEvent::SearchWord(word));
                return Typed::Leave;
            }
            "escape" => {
                self.peek = None;
                cx.emit(ReviewEvent::Changed);
                return Typed::Leave;
            }
            _ => return Typed::Kept,
        }
        cx.emit(ReviewEvent::Changed);
        Typed::Kept
    }

    /// ⌃G: `Line` in the file's head, empty.
    pub fn open_file_line(&mut self) {
        if self.file_lines_shown() {
            self.peek = None;
            self.file_line = Some(String::new());
        }
    }

    /// A key while `Line` has the keyboard: digits and one `:` before a column, ⌫, ↵ goes there (past the end, the
    /// last line; a column past the line's end, its end) with the caret at the column, esc closes.
    fn file_line_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> Typed {
        let Some(typed) = self.file_line.as_mut() else {
            return Typed::Leave;
        };
        // `:` as typed (⇧; on a US keyboard).
        let colon = key.key == ":"
            || key.key_char.as_deref() == Some(":")
            || (key.key == ";" && key.modifiers.shift);
        let k = if colon { ":" } else { key.key.as_str() };
        match k {
            "backspace" => {
                typed.pop();
            }
            "enter" => {
                let count = self.file_view.as_ref().map_or(0, |v| v.lines.len());
                if let Some((n, column)) = line_and_column(typed)
                    && count > 0
                {
                    let line = n.min(count) - 1;
                    self.file_line = None;
                    self.file_target = Some(line);
                    self.scroll_file_to(line);
                    if let (Some(column), Some(view)) = (column, self.file_view.as_ref()) {
                        let at = (view.plain.borrow().get(line))
                            .map_or(0, |text| byte_of_column(text, column));
                        cx.emit(ReviewEvent::Caret(artoo_ui_kit::markdown::TextSel {
                            note: crate::file_view::selects_as(&view.path).into(),
                            anchor: (line, at),
                            head: (line, at),
                            texts: view.plain.clone(),
                        }));
                    }
                    cx.emit(ReviewEvent::Changed);
                    return Typed::Leave;
                }
            }
            "escape" => {
                self.file_line = None;
                cx.emit(ReviewEvent::Changed);
                return Typed::Leave;
            }
            _ if k.len() == 1
                && (k.as_bytes()[0].is_ascii_digit()
                    || (k == ":" && !typed.is_empty() && !typed.contains(':')))
                && !key.modifiers.platform
                && !key.modifiers.control
                && typed.len() < 14 =>
            {
                typed.push_str(k)
            }
            _ => return Typed::Kept,
        }
        cx.emit(ReviewEvent::Changed);
        Typed::Kept
    }

    /// The `Line` field for the file's head, while it is open.
    pub fn render_file_line(&self, active: bool) -> Option<gpui::Div> {
        let typed = self.file_line.as_ref()?;
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .h(px(24.))
                .px(px(8.))
                .rounded(px(4.))
                .border_1()
                .border_color(if active {
                    theme::focus()
                } else {
                    theme::island_border()
                })
                .child(div().text_color(theme::faint()).child("Line"))
                .child(
                    div()
                        .min_w(px(36.))
                        .font_family(theme::code_font())
                        .text_color(theme::text())
                        .child(if typed.is_empty() {
                            " ".to_owned()
                        } else {
                            typed.clone()
                        }),
                )
                .child(keys::caps("↵", keys::On::Plain)),
        )
    }
}

/// `Line`'s text read as a 1-based line and, after a `:`, a 1-based column: `42`, `42:7`; `42:` is the line.
fn line_and_column(typed: &str) -> Option<(usize, Option<usize>)> {
    let (line, column) = match typed.split_once(':') {
        Some((line, "")) => (line, None),
        Some((line, column)) => (line, Some(column.parse::<usize>().ok().filter(|&c| c > 0)?)),
        None => (typed, None),
    };
    let line = line.parse::<usize>().ok().filter(|&n| n > 0)?;
    Some((line, column))
}

/// The byte before the 1-based `column`th character of `text`; past its end, its end.
fn byte_of_column(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column - 1)
        .map_or(text.len(), |(at, _)| at)
}

/// How tall the peek's card is with its gaps (`peek_card`), in the kit's points: its head, a row a declaration (or
/// the one saying there is none yet), its foot.
pub fn peek_height(peek: &Peek) -> f32 {
    let rows = peek.found.as_ref().map_or(1, |f| f.len().max(1));
    8. + 2. + 30. + 28. * rows as f32 + 28.
}

/// The peek's card under its line, `left` in from the row's start: `By name · 3`, the declarations, the keys.
pub fn peek_card(look: &PeekLook, left: f32, this: WeakEntity<Review>) -> AnyElement {
    let count = match &look.found {
        Some(found) => found.len().to_string(),
        None => "…".to_owned(),
    };
    let head = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(30.))
        .px(px(10.))
        .border_b_1()
        .border_color(theme::island_border())
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(format!("By name · {count}")),
        )
        .child(
            div()
                .font_family(theme::code_font())
                .text_color(theme::faint())
                .child(look.word.clone()),
        )
        .child(div().flex_1())
        .child(div().text_color(theme::faint()).child("⌘-click"));
    let rows = (look.found.iter().flatten().enumerate()).map(|(ix, d)| {
        let (dir, name) = match d.path.rsplit_once('/') {
            Some((dir, name)) => (dir.to_owned(), name.to_owned()),
            None => (String::new(), d.path.clone()),
        };
        let open = this.clone();
        // clip: one row a declaration; its file:line and folder stay whole, its line's text gives way.
        div()
            .id(("peek-row", ix))
            .flex()
            .items_center()
            .gap(px(12.))
            .min_h(px(28.))
            .px(px(10.))
            .whitespace_nowrap()
            .overflow_hidden()
            .cursor_pointer()
            .when(ix == look.sel, |s| s.bg(theme::selected()))
            .hover(|s| s.bg(theme::hover()))
            .child(
                div()
                    .flex_none()
                    .font_family(theme::code_font())
                    .text_color(if d.path == look.here {
                        theme::accent()
                    } else {
                        theme::text()
                    })
                    .child(format!("{name}:{}", d.line)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .font_family(theme::code_font())
                    .text_color(theme::code_text())
                    // clip: the declaration's line, its file:line is whole beside it.
                    .child(d.text.clone()),
            )
            .child(div().flex_none().text_color(theme::faint()).child(dir))
            .on_click(move |ev, _, cx| {
                let keep = ev.modifiers().platform;
                open.update(cx, |rv, cx| rv.open_declared(ix, keep, cx))
                    .ok();
            })
    });
    let empty = match &look.found {
        None => Some("Reading the repository's names…"),
        Some(found) if found.is_empty() => Some("No declaration by this name"),
        Some(_) => None,
    };
    let foot = div()
        .flex()
        .items_center()
        .gap(px(14.))
        .h(px(28.))
        .px(px(10.))
        .border_t_1()
        .border_color(theme::island_border())
        .text_color(theme::faint())
        .child(hint("↵", "open"))
        .child(hint("⌘↵", "new tab"))
        .child(hint("⇧F12", "search the word"))
        .child(div().flex_1())
        .child(keys::caps("esc", keys::On::Plain));
    div()
        .py(px(4.))
        .pl(px(left))
        .child(
            div()
                .w(px(620.))
                .max_w_full()
                .overflow_hidden()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::border())
                .bg(theme::panel())
                .shadow_md()
                .font_family(theme::ui_font())
                .text_size(px(12.))
                .whitespace_normal()
                .child(head)
                .children(rows)
                .children(empty.map(|text| {
                    div()
                        .h(px(28.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .text_color(theme::faint())
                        .child(text)
                }))
                .child(foot),
        )
        .into_any_element()
}

fn hint(key: &'static str, what: &'static str) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(px(5.))
        .child(keys::caps(key, keys::On::Plain))
        .child(what)
}

/// The peek as one of the tab's inputs: ↑ ↓ ↵ ⇧F12 esc are its own; one left closes.
pub fn peek_input(review: Entity<Review>) -> Input {
    let (open, key, left) = (review.downgrade(), review.downgrade(), review.downgrade());
    Input {
        id: PEEK_INPUT,
        open: Rc::new(move |cx| open.upgrade().is_some_and(|rv| rv.read(cx).peek.is_some())),
        key: Rc::new(move |event, _, cx| {
            (key.update(cx, |rv, cx| rv.peek_key(&event.keystroke, cx))).unwrap_or(Typed::Kept)
        }),
        left: Rc::new(move |cx: &mut App| {
            left.update(cx, |rv, cx| {
                if rv.close_peek() {
                    cx.emit(ReviewEvent::Changed);
                }
            })
            .ok();
        }),
        selected: Rc::new(|_| None),
        caret: Rc::new(|_| true),
    }
}

/// `Line` as one of the tab's inputs: one left closes.
pub fn line_input(review: Entity<Review>) -> Input {
    let (open, key, left) = (review.downgrade(), review.downgrade(), review.downgrade());
    Input {
        id: LINE_INPUT,
        open: Rc::new(move |cx| (open.upgrade()).is_some_and(|rv| rv.read(cx).file_line.is_some())),
        key: Rc::new(move |event, _, cx| {
            (key.update(cx, |rv, cx| rv.file_line_key(&event.keystroke, cx))).unwrap_or(Typed::Kept)
        }),
        left: Rc::new(move |cx: &mut App| {
            left.update(cx, |rv, cx| {
                if rv.file_line.take().is_some() {
                    cx.emit(ReviewEvent::Changed);
                }
            })
            .ok();
        }),
        selected: Rc::new(|_| None),
        caret: Rc::new(|_| false),
    }
}

#[cfg(test)]
mod tests {
    use super::{byte_of_column, line_and_column, up_to_date};

    #[test]
    fn line_reads_a_line_and_a_column() {
        assert_eq!(line_and_column("42"), Some((42, None)));
        assert_eq!(line_and_column("42:7"), Some((42, Some(7))));
        assert_eq!(line_and_column("42:"), Some((42, None)));
        assert_eq!(line_and_column("0"), None);
        assert_eq!(line_and_column("42:0"), None);
        assert_eq!(line_and_column(":7"), None);
        assert_eq!(byte_of_column("    val é = 1", 5), 4);
        assert_eq!(byte_of_column("val é = 1", 6), 6);
        assert_eq!(byte_of_column("val", 9), 3);
    }
    use artoo_core::symbols::Names;

    /// What going to a name costs on this repository (#207: 40 ms and 3.5 MB measured): its names are read from its
    /// files and the core's own functions are found; read again with nothing changed, nothing is read.
    #[test]
    fn this_repositorys_names_are_read_and_found() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let started = std::time::Instant::now();
        let names = up_to_date(&root, Names::default());
        let took = started.elapsed();
        let again = std::time::Instant::now();
        let names = up_to_date(&root, names);
        eprintln!(
            "names: {} in {} files, {} lines, {took:?}; again {:?}",
            names.len(),
            names.files,
            names.lines,
            again.elapsed()
        );
        let found = names.of("word_at", "crates/core/src/symbols.rs");
        assert_eq!(
            found.first().map(|d| d.path.as_str()),
            Some("crates/core/src/symbols.rs")
        );
    }

    #[test]
    fn a_changed_file_is_read_again_and_a_gone_one_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        let file = dir.path().join("Login.kt");
        std::fs::write(&file, "class LoginState").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("Login.kt")).unwrap();
        index.write().unwrap();
        let names = up_to_date(dir.path(), Names::default());
        assert_eq!(names.of("LoginState", "").len(), 1);
        // Another length: read again, whatever the clock says.
        std::fs::write(&file, "\nclass LoginScreen").unwrap();
        let names = up_to_date(dir.path(), names);
        assert!(names.of("LoginState", "").is_empty());
        assert_eq!(names.of("LoginScreen", "")[0].line, 2);
        std::fs::remove_file(&file).unwrap();
        let names = up_to_date(dir.path(), names);
        assert!(names.is_empty());
    }
}
