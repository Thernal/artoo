//! Going to a name with no language server, going to a line, and where you are, in Code's file (#207).
//!
//! The repository's declarations by name (`artoo_core::symbols::Names`), read by keyword from its files as the
//! Outline reads one file's, are built in the background at the first ⌘-click or F12 on a name — never at start —
//! and brought up to date at each one after: only the files changed since are read again. The peek opens under the line, headed `By name · 3`: the file read's own declaration first, then the
//! others; ↑ ↓ choose, ↵ opens it in Code, ⇧F12 searches the word in the repository, esc closes. A name declared
//! once, elsewhere, is opened at once. ⌃G puts `Line` in the file's head, ↵ goes there. Each is one of the tab's
//! inputs (`peek_input`, `line_input`).

use crate::review::{Review, ReviewEvent};
use artoo_core::symbols::{self, Declared, Names};
use artoo_shell_api::{Input, Typed};
use artoo_ui_kit::{keys, px, theme};
use gpui::{AnyElement, App, Context, Entity, Keystroke, WeakEntity, div, prelude::*};
use std::rc::Rc;
use std::sync::Arc;

/// The peek's place among the tab's inputs.
pub const PEEK_INPUT: &str = artoo_review_api::PEEK_INPUT;
/// The `Line` field's place among the tab's inputs.
pub const LINE_INPUT: &str = artoo_review_api::LINE_INPUT;

/// The declarations of a name, open under its line.
pub struct Peek {
    pub word: String,
    /// The line (0-based) it is under.
    pub line: usize,
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
        let Some(word) = (plain.get(line)).and_then(|t| symbols::word_at(t, at)) else {
            return false;
        };
        let word = word.to_owned();
        drop(plain);
        self.file_line = None;
        self.peek = Some(Peek {
            word,
            line,
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
                    cx.emit(ReviewEvent::Go { path, line });
                    return;
                }
                peek.found = Some(found);
            }
            _ if self.names_reading.is_some() => {}
            _ => {
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
                        }
                        rv.fill_peek(cx);
                        cx.emit(ReviewEvent::Changed);
                    })
                    .ok();
                }));
            }
        }
    }

    /// The peek closes.
    pub fn close_peek(&mut self) -> bool {
        self.peek.take().is_some()
    }

    /// Declaration `ix` of the peek opened: the peek closes, the tab opens it.
    pub fn open_declared(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(peek) = self.peek.take() else {
            return;
        };
        if let Some(d) = peek.found.as_ref().and_then(|f| f.get(ix)) {
            cx.emit(ReviewEvent::Go {
                path: d.path.clone(),
                line: d.line,
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
                self.open_declared(at, cx);
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

    /// A key while `Line` has the keyboard: digits, ⌫, ↵ goes there (past the end, the last line), esc closes.
    fn file_line_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> Typed {
        let Some(typed) = self.file_line.as_mut() else {
            return Typed::Leave;
        };
        let k = key.key.as_str();
        match k {
            "backspace" => {
                typed.pop();
            }
            "enter" => {
                let count = self.file_view.as_ref().map_or(0, |v| v.lines.len());
                if let Ok(n) = typed.parse::<usize>()
                    && n > 0
                    && count > 0
                {
                    let line = n.min(count) - 1;
                    self.file_line = None;
                    self.file_target = Some(line);
                    self.scroll_file_to(line);
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
                && k.as_bytes()[0].is_ascii_digit()
                && !key.modifiers.platform
                && !key.modifiers.control
                && typed.len() < 7 =>
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
            .on_click(move |_, _, cx| {
                open.update(cx, |rv, cx| rv.open_declared(ix, cx)).ok();
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
    use super::up_to_date;
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
