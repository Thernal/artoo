//! The commits' search (⌘⇧F): what is typed, the commits it finds among those listed, and the paths each commit
//! touched, read in the background so `path:` can be judged. The matching itself is the core's `search`. Its
//! field (`field`), its words when nothing matches (`no_match`), the field as a part's (`input`).

use crate::history::{Act, Acts, Event, History, Selection};
use artoo_core::git::Repo;
use artoo_core::search::{self, Query};
use artoo_shell_api::{Input, Typed};
use artoo_ui_kit::input::{self, Area, Edit};
use artoo_ui_kit::{ROW_RADIUS, icons, keys, px, theme};
use git2::Oid;
use gpui::{
    App, ClickEvent, Context, Div, Entity, HighlightStyle, Keystroke, ScrollStrategy, SharedString,
    StyledText, Task, WeakEntity, div, prelude::*,
};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

/// The search's field as an input of the tab's (`artoo_shell_api::Input`).
pub use artoo_branches_api::SEARCH_INPUT as INPUT;

/// Commits whose paths are read per background step.
const PATH_CHUNK: usize = 250;

#[derive(Default)]
pub struct Find {
    /// The text in the field; `None` while the field is not active.
    pub text: Option<String>,
    /// The field's caret, selection and layout.
    pub area: Area,
    pub query: Query,
    /// Indices into `History::commits` of the commits that match; `None` shows every commit.
    pub shown: Option<Vec<usize>>,
    /// Paths each commit touched, read in the background after a branch loads.
    paths: HashMap<Oid, Vec<String>>,
    /// Paths are still being read.
    pub indexing: bool,
    task: Option<Task<()>>,
}

impl Find {
    /// A new repository: nothing known about its commits.
    pub fn reset(&mut self) {
        self.paths.clear();
        self.indexing = false;
        self.task = None;
        self.shown = None;
    }
}

/// Qualifiers offered as tags under the field.
const QUALIFIERS: [&str; 3] = ["author:", "path:", "since:"];

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl History {
    /// The search's field open, empty unless it is open already.
    pub fn open_find(&mut self) {
        if self.find.text.is_none() {
            self.find.area = Area::default();
            self.find.text = Some(String::new());
        }
    }

    /// A key typed in the search's field: what it found as the text changes. What the key did.
    pub fn find_key(&mut self, key: &Keystroke, cx: &mut App) -> Edit {
        let Some(mut text) = self.find.text.clone() else {
            return Edit::Ignored;
        };
        let edit = input::edit(&mut text, &mut self.find.area.edit, key, cx);
        if matches!(edit, Edit::Changed) {
            self.set_find(text);
        }
        edit
    }

    /// The search closed: every commit listed again, the one selected in the middle of the list.
    pub fn close_find(&mut self) {
        self.find.text = None;
        self.refilter();
        if let Selection::Commit(ix) = self.selection
            && let Some(row) = self.row_of(ix)
        {
            self.commit_scroll
                .scroll_to_item(row, ScrollStrategy::Center);
        }
    }

    /// The search's text `text`: the commits it finds, the one selected kept in view.
    pub fn set_find(&mut self, text: String) {
        self.find.text = Some(text);
        self.refilter();
        if let Selection::Commit(ix) = self.selection
            && let Some(row) = self.row_of(ix)
        {
            self.commit_scroll
                .scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }

    /// Rows in the commit list.
    pub fn commit_rows(&self) -> usize {
        self.find
            .shown
            .as_ref()
            .map_or(self.commits.len(), Vec::len)
    }

    /// The index in `commits` shown at list row `row`.
    pub fn commit_at(&self, row: usize) -> usize {
        self.find.shown.as_ref().map_or(row, |s| s[row])
    }

    /// The list row of commit `ix`, if it is shown.
    pub fn row_of(&self, ix: usize) -> Option<usize> {
        match &self.find.shown {
            None => Some(ix),
            Some(shown) => shown.iter().position(|&i| i == ix),
        }
    }

    /// Filters `commits` by the field's text; the selection stays where it is.
    pub fn refilter(&mut self) {
        let text = self.find.text.clone().unwrap_or_default();
        self.find.query = Query::parse(&text, now());
        self.find.shown = (!self.find.query.is_empty()).then(|| {
            self.commits
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    let paths = self.find.paths.get(&c.id).map(Vec::as_slice);
                    // A path condition cannot be judged before the paths are known.
                    self.find.query.matches(c, paths)
                })
                .map(|(ix, _)| ix)
                .collect()
        });
    }

    /// Reads the paths of every listed commit, a chunk at a time; the filter updates as they arrive.
    pub fn index_paths(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let ids: Vec<Oid> = self
            .commits
            .iter()
            .map(|c| c.id)
            .filter(|id| !self.find.paths.contains_key(id))
            .collect();
        if ids.is_empty() {
            self.find.indexing = false;
            return;
        }
        self.find.indexing = true;
        self.find.task = Some(cx.spawn(async move |this, cx| {
            for chunk in ids.chunks(PATH_CHUNK) {
                let chunk = chunk.to_vec();
                let root = root.clone();
                let read = cx
                    .background_executor()
                    .spawn(async move {
                        Repo::open(&root)
                            .map(|repo| repo.changed_paths(&chunk))
                            .unwrap_or_default()
                    })
                    .await;
                let alive = this.update(cx, |h, cx| {
                    h.find.paths.extend(read);
                    if h.find.query.needs_paths() || h.find.text.is_some() {
                        h.refilter();
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            this.update(cx, |h, cx| {
                h.find.indexing = false;
                cx.notify();
            })
            .ok();
        }));
    }
}

/// A commit's subject, the `words` searched for marked in it.
pub fn summary_text(summary: SharedString, words: &[String]) -> StyledText {
    let ranges = search::highlights(&summary, words);
    StyledText::new(summary).with_highlights(ranges.into_iter().map(|range| {
        (
            range,
            HighlightStyle {
                color: Some(theme::warning().into()),
                background_color: Some(theme::warning_bg().into()),
                ..Default::default()
            },
        )
    }))
}

/// The search's field over the commits, the qualifiers' tags under it while it has the keyboard (`active`), and
/// how many it found; a click in it asks the tab for the keyboard (`Act::FindField`). `this` is the history, which
/// a click on a tag or in the text changes.
pub fn field(h: &History, active: bool, this: WeakEntity<History>, acts: &Acts) -> Div {
    let text = h.find.text.clone().unwrap_or_default();
    let on_click = acts.clone();
    let field = div()
        .id("find")
        .flex_none()
        .flex()
        .items_center()
        .gap_2()
        .mx(px(6.))
        .mb(px(6.))
        .px_2()
        .h(px(28.))
        .rounded(px(ROW_RADIUS))
        .border_2()
        .border_color(if active {
            theme::focus()
        } else {
            theme::island_border()
        })
        .bg(theme::base())
        .cursor_text()
        .on_click(move |_: &ClickEvent, _, cx| on_click(Act::FindField, cx))
        .child(
            icons::icon("search")
                .size(px(14.))
                .text_color(theme::faint()),
        )
        .child(h.find.area.field_of(
            &text,
            active,
            "Search commits",
            false,
            |h: &mut History| Some(&mut h.find.area),
            this.clone(),
        ))
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(keys::text(if text.is_empty() { "⌘⇧F" } else { "esc" }).into_owned()),
        );
    let tags = active.then(|| {
        div()
            .flex_none()
            .flex()
            .gap_1()
            .px(px(8.))
            .pb(px(6.))
            .children(QUALIFIERS.into_iter().map(|q| {
                let this = this.clone();
                div()
                    .id(q)
                    .px(px(6.))
                    .rounded_full()
                    .border_1()
                    .border_color(theme::faint())
                    .text_size(px(10.))
                    .text_color(theme::muted())
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::text()))
                    .child(q)
                    .on_click(move |_: &ClickEvent, _, cx| {
                        this.update(cx, |h, cx| {
                            let mut text = h.find.text.clone().unwrap_or_default();
                            if !text.is_empty() && !text.ends_with(' ') {
                                text.push(' ');
                            }
                            text.push_str(q);
                            h.set_find(text);
                            cx.notify();
                        })
                        .ok();
                    })
            }))
    });
    let count = h.find.shown.as_ref().map(|shown| {
        let mut line = format!("{} of {} commits", shown.len(), h.commits.len());
        // Older history still being read for it (#25).
        if h.older.is_some() {
            line.push_str(" · reading older ones…");
        } else if h.find.indexing {
            line.push_str(" · reading paths…");
        }
        div()
            .flex_none()
            .px(px(14.))
            .pb(px(4.))
            .text_size(px(11.))
            .text_color(theme::faint())
            .child(line)
    });
    div()
        .flex()
        .flex_col()
        .child(field)
        .children(tags)
        .children(count)
}

/// Shown instead of the list when nothing matches.
pub fn no_match(h: &History) -> Div {
    let text = h.find.text.clone().unwrap_or_default();
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .px_4()
        .text_color(theme::muted())
        .child(format!("No commit matches “{text}”."))
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(match h.older {
                    Some(_) => format!(
                        "Searched the {} newest commits; reading older ones…",
                        h.commits.len()
                    ),
                    None => format!("Searched all {} commits.", h.commits.len()),
                }),
        )
}

/// The search's field as the tab's input: its keys edit it, ↵ shows the first commit found (`Event::Picked`), esc
/// lets the keyboard go; left empty, it closes.
pub fn input(history: Entity<History>) -> Input {
    let (open, key, left, selected) = (
        history.downgrade(),
        history.downgrade(),
        history.downgrade(),
        history.downgrade(),
    );
    Input {
        id: INPUT,
        open: Rc::new(move |cx| {
            open.upgrade()
                .is_some_and(|h| h.read(cx).find.text.is_some())
        }),
        key: Rc::new(move |event, _, cx| {
            key.update(cx, |h, cx| match h.find_key(&event.keystroke, cx) {
                Edit::Changed | Edit::Selected => {
                    cx.notify();
                    Typed::Kept
                }
                Edit::Enter { .. } => {
                    if h.commit_rows() > 0 {
                        cx.emit(Event::Picked(h.commit_at(0)));
                    }
                    Typed::Kept
                }
                Edit::Escape => Typed::Leave,
                Edit::Ignored => Typed::Kept,
            })
            .unwrap_or(Typed::Kept)
        }),
        left: Rc::new(move |cx| {
            left.update(cx, |h, cx| {
                if h.find.text.as_deref() == Some("") {
                    h.close_find();
                    cx.notify();
                }
            })
            .ok();
        }),
        selected: Rc::new(move |cx| {
            let h = selected.upgrade()?;
            h.read(cx).find.area.edit.selected_text().map(str::to_owned)
        }),
        caret: Rc::new(|_| false),
    }
}
