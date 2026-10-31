//! Commit search: the field above the commit list, its keyboard handling, and the filtered view of
//! `History::commits`. The matching itself is `crate::search`.

use super::input::{self, Edit};
use super::px;
use super::typing::{self, Input};
use super::{COMMIT_PAGE, Older, Workspace, theme};
use crate::git::Repo;
use crate::search::{self, Query};
use artoo_ui_kit::controls::chip;
use git2::Oid;
use gpui::{
    App, ClickEvent, Context, HighlightStyle, KeyDownEvent, SharedString, StyledText, Task, div,
    prelude::*,
};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Qualifiers offered as tags under the field.
const QUALIFIERS: [&str; 3] = ["author:", "path:", "since:"];
/// Commits whose paths are read per background step.
const PATH_CHUNK: usize = 250;

#[derive(Default)]
pub struct Find {
    /// The text in the field; `None` while the field is not active.
    pub text: Option<String>,
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

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Workspace {
    /// Rows in the commit list.
    pub(super) fn commit_rows(&self, cx: &App) -> usize {
        self.find
            .shown
            .as_ref()
            .map_or(self.history.read(cx).commits.len(), Vec::len)
    }

    /// The index in `commits` shown at list row `row`.
    pub(super) fn commit_at(&self, row: usize) -> usize {
        self.find.shown.as_ref().map_or(row, |s| s[row])
    }

    /// The list row of commit `ix`, if it is shown.
    pub(super) fn row_of(&self, ix: usize) -> Option<usize> {
        match &self.find.shown {
            None => Some(ix),
            Some(shown) => shown.iter().position(|&i| i == ix),
        }
    }

    /// The branch's older history, a page more (#25): as the list is scrolled near its end, and
    /// on and on while a search has not found what it looks for. A page is as long as what is
    /// listed, so a search through a long history reads it in a few.
    pub(super) fn load_older_commits(&mut self, cx: &mut Context<Self>) {
        let history = self.history.read(cx);
        let (Some(root), Some(older)) = (self.root.clone(), history.older.as_ref()) else {
            return;
        };
        if older.task.is_some() {
            return;
        }
        let (tip, skip) = (older.tip, history.commits.len());
        let count = skip.max(COMMIT_PAGE);
        let task = cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { Repo::open(&root)?.log(tip, skip, count, None) })
                .await;
            this.update(cx, |this, cx| {
                // Another branch, or the history read again meanwhile: not these.
                if this
                    .history
                    .read(cx)
                    .older
                    .as_ref()
                    .is_none_or(|o| o.tip != tip)
                    || this.history.read(cx).commits.len() != skip
                {
                    return;
                }
                match read {
                    Ok(page) => {
                        let more = page.len() >= count;
                        this.history.update(cx, |h, _| {
                            h.commits.extend(page);
                            h.older = more.then_some(Older { tip, task: None });
                        });
                        this.refilter(cx);
                        this.index_paths(cx);
                    }
                    Err(e) => {
                        this.history.update(cx, |h, _| h.older = None);
                        this.error = Some(format!("{e:#}").into());
                    }
                }
                cx.notify();
            })
            .ok();
        });
        self.history.update(cx, |h, _| {
            if let Some(older) = h.older.as_mut() {
                older.task = Some(task);
            }
        });
    }

    /// Reads a page more of history once this frame is drawn, unless one is on its way.
    pub(super) fn want_older_commits(&self, cx: &mut Context<Self>) {
        if self
            .history
            .read(cx)
            .older
            .as_ref()
            .is_none_or(|o| o.task.is_some())
        {
            return;
        }
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            this.update(cx, |this, cx| this.load_older_commits(cx)).ok();
        });
    }

    /// Filters `commits` by the field's text; the selection stays where it is.
    pub(super) fn refilter(&mut self, cx: &App) {
        let text = self.find.text.clone().unwrap_or_default();
        self.find.query = Query::parse(&text, now());
        self.find.shown = (!self.find.query.is_empty()).then(|| {
            self.history
                .read(cx)
                .commits
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

    pub(super) fn start_find(&mut self, cx: &mut Context<Self>) {
        if self.find.text.is_none() {
            self.reset_field_edit();
        }
        self.settings_open = false;
        if self.find.text.is_none() {
            self.find.text = Some(String::new());
        }
        self.take_keyboard(Input::CommitSearch, cx);
        cx.notify();
    }

    pub(super) fn close_find(&mut self, cx: &mut Context<Self>) {
        self.find.text = None;
        self.refilter(cx);
        if let Selection::Commit(ix) = self.history.read(cx).selection
            && let Some(row) = self.row_of(ix)
        {
            self.commit_scroll
                .scroll_to_item(row, gpui::ScrollStrategy::Center);
        }
        cx.notify();
    }

    fn set_find_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.find.text = Some(text);
        self.refilter(cx);
        if let Selection::Commit(ix) = self.history.read(cx).selection
            && let Some(row) = self.row_of(ix)
        {
            self.commit_scroll
                .scroll_to_item(row, gpui::ScrollStrategy::Nearest);
        }
        cx.notify();
    }

    pub(super) fn step_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.review.read(cx).stack.is_some() {
            if let Some(at) = self.review.read(cx).next_stack_match(forward) {
                self.go_to_stack_match(at, cx);
            }
            return;
        }
        if self.review.update(cx, |rv, _| rv.step_match(forward)) {
            cx.notify();
        }
    }

    /// F7 / ⇧F7: the next (or previous) change in the open file.
    pub(super) fn step_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        // A document read as a page: its changed blocks (#11).
        if self.step_page(forward, cx) {
            return;
        }
        if self.review.update(cx, |rv, _| rv.step_change(forward)) {
            cx.notify();
        }
    }

    /// A key: to the input with the keyboard (#168), else the keys of what is on screen.
    pub(super) fn find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_keyboard(cx);
        match self.typing(cx) {
            Some(Input::FindFiles) => return self.ffind_key(event, cx),
            Some(Input::Palette) => return self.palette_key(event, window, cx),
            Some(Input::OpenAsk) => return self.open_ask_key(event, cx),
            Some(Input::AgentPick) => return self.agent_pick_key(event, cx),
            Some(Input::Overlay) => return self.overlay_key(event, window, cx),
            Some(Input::Beside) => return self.beside_key(event, cx),
            Some(Input::Merge) => return self.merge_key(event, cx),
            Some(Input::Reactions) => return self.reaction_key(event, cx),
            Some(Input::Submit) => return self.submit_key(event, cx),
            Some(Input::NewRequest) => return self.newreq_key(event, cx),
            Some(Input::GoLine) => return self.goline_key(event, cx),
            Some(Input::Tool) => return self.tool_key(event, window, cx),
            Some(Input::Compose) => return self.compose_key(event, cx),
            Some(Input::Field) => {
                self.filter_key(event, cx);
                return;
            }
            Some(Input::DiffFind) => return self.dfind_key(event, cx),
            Some(Input::CommitSearch) => return self.search_key(event, cx),
            None => {}
        }
        let key = &event.keystroke;
        // ⌫ on the draft picked in the Agents window deletes it; Undo is in its toast (#76).
        if key.key == "backspace"
            && self.tool() == Some(super::tools::Tool::Agents)
            && let Some(id) = self.agent_row.filter(|id| {
                self.comments
                    .iter()
                    .any(|c| c.id == *id && c.state == crate::review::State::Draft)
            })
        {
            return self.delete_draft(id, cx);
        }
        // Esc closes the innermost thing — a card, menu or popover, the find bar, a filter, the
        // tool window with the keyboard — then goes one page up (#71).
        if key.key == "escape" {
            if self.review.read(cx).done_card {
                self.review.update(cx, |rv, _| rv.done_card = false);
                cx.notify();
            } else if self.label_edit.is_some() {
                self.close_label(cx);
            } else if self.overlay_open() {
                self.close_overlay(cx);
            } else if self.shortcuts {
                self.shortcuts = false;
                cx.notify();
            } else if self.ctx_menu.is_some() {
                self.close_menu(cx);
            } else if self.review.read(cx).goline.is_some() {
                self.close_goline(cx);
                cx.notify();
            } else if self.review.read(cx).dfind.is_some() {
                self.close_dfind(cx);
            } else if self.find.text.is_some() {
                self.close_find(cx);
            } else if self.side_drawer() {
                self.close_drawer(cx);
            } else if !self.nav.picked.is_empty() {
                self.nav.picked.clear();
                cx.notify();
            } else if self.close_setup(cx) || (self.settings_open && self.theme_edit_escape(cx)) {
            } else if self.settings_open {
                self.settings_open = false;
                cx.notify();
            } else if self.note_sel.take().is_some()
                || self.review.update(cx, |rv, _| rv.sel.take().is_some())
            {
                cx.notify();
            } else if self.tool().is_some() && self.zone == super::zones::Zone::Tool {
                self.set_tool(None, cx);
            } else if self.clear_filter(cx) {
                cx.notify();
            } else if self.unpick_compare(cx) {
            } else {
                self.go_up(cx);
            }
            return;
        }
        // 0…9 answer the agent's form in view (#10 C).
        if key.modifiers.number_of_modifiers() == 0
            && let Some(digit) = key.key.parse::<usize>().ok().filter(|d| *d < 10)
        {
            self.answer_by_key(digit, cx);
        }
    }

    /// Esc after esc: the filter that was left with text, of the list on screen, is cleared.
    fn clear_filter(&mut self, cx: &mut App) -> bool {
        if !self.review.read(cx).pfilter.is_empty()
            && self.review.read(cx).diff.is_some()
            && self
                .space()
                .is_some_and(|s| s != super::spaces::Space::Code)
        {
            self.review.update(cx, |rv, _| rv.pfilter.clear());
            return true;
        }
        if !self.bfilter.is_empty() && self.at_root_list() {
            self.bfilter.clear();
            return true;
        }
        false
    }

    /// Typing into the find bar over the diff; esc lets the keyboard go, what was typed kept.
    fn dfind_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let Some(mut text) = self.review.read(cx).dfind.clone() else {
            return;
        };
        // ⌥R: the text as a regular expression, or as typed again (#113).
        if key.modifiers.alt && key.key == "r" {
            return self.toggle_dfind_regex(cx);
        }
        match input::edit(&mut text, &mut self.field_edit, key, cx) {
            Edit::Changed => self.set_dfind(text, cx),
            Edit::Selected => cx.notify(),
            Edit::Enter { shift } => self.step_match(!shift, cx),
            Edit::Escape => self.leave_input(cx),
            Edit::Ignored => {}
        }
    }

    /// Typing into the commits' search; ↵ opens the first commit found.
    fn search_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(mut text) = self.find.text.clone() else {
            return;
        };
        match input::edit(&mut text, &mut self.field_edit, &event.keystroke, cx) {
            Edit::Changed => self.set_find_text(text, cx),
            Edit::Selected => cx.notify(),
            Edit::Enter { .. } => {
                if self.commit_rows(cx) > 0 {
                    self.select_commit(self.commit_at(0), cx);
                }
            }
            Edit::Escape => self.leave_input(cx),
            Edit::Ignored => {}
        }
    }

    // ---- find in the diff ----------------------------------------------------------------

    /// ⌘F: a find bar over the open diff, like the page find of a browser or an IDE.
    pub(super) fn start_dfind(&mut self, cx: &mut Context<Self>) {
        if self.review.read(cx).dfind.is_none() {
            self.reset_field_edit();
        }
        self.settings_open = false;
        if self.review.read(cx).dfind.is_none() {
            self.review
                .update(cx, |rv, _| rv.dfind = Some(String::new()));
        }
        self.take_keyboard(Input::DiffFind, cx);
        cx.notify();
    }

    /// `.*` in ⌘F's bar: its text read as a regular expression, or as typed.
    pub(super) fn toggle_dfind_regex(&mut self, cx: &mut Context<Self>) {
        self.review
            .update(cx, |rv, _| rv.dfind_regex = !rv.dfind_regex);
        let text = self.review.read(cx).dfind.clone().unwrap_or_default();
        self.set_dfind(text, cx);
    }

    pub(super) fn close_dfind(&mut self, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| rv.close_find());
        cx.notify();
    }

    pub(super) fn set_dfind(&mut self, text: String, cx: &mut Context<Self>) {
        self.review.update(cx, |rv, _| rv.set_find(text));
        if self.review.read(cx).stack.is_some() {
            // The folded files are built to be searched too.
            self.build_stack(cx);
            if let Some(at) = self.review.read(cx).first_stack_match() {
                self.go_to_stack_match(at, cx);
            }
        }
        cx.notify();
    }

    /// The find bar above the diff: the text, "2 of 4 lines", previous, next, close.
    pub(super) fn render_dfind(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let text = self.review.read(cx).dfind.clone()?;
        let active = self.has_keyboard(Input::DiffFind, cx);
        let count = if let Some(error) = &self.review.read(cx).dfind_error {
            format!("Not a regular expression: {error}")
        } else if self.review.read(cx).dterms.is_empty() {
            String::new()
        } else if let Some(count) = self.review.read(cx).stack_match_count() {
            count
        } else if self.review.read(cx).matches.is_empty() {
            "No matches".to_owned()
        } else {
            format!(
                "{} of {} lines",
                self.review.read(cx).match_at + 1,
                self.review.read(cx).matches.len()
            )
        };
        let step = |id: &'static str, label: &'static str, forward: bool| {
            chip(id, label, false).on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.step_match(forward, cx)),
            )
        };
        let bar = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .px(px(10.))
            .py(px(6.))
            .border_b_1()
            .border_color(theme::island_border())
            .child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(26.))
                    .px_2()
                    .rounded(px(super::ROW_RADIUS))
                    .border_2()
                    .border_color(if active {
                        theme::focus()
                    } else {
                        theme::island_border()
                    })
                    .bg(theme::base())
                    .cursor_text()
                    .child(
                        super::icons::icon("search")
                            .size(px(14.))
                            .text_color(theme::faint()),
                    )
                    .child(self.field_text(&text, active, "Find in this diff", cx))
                    .child({
                        use super::hover::Hover;
                        chip("dfind-regex", ".*", self.review.read(cx).dfind_regex)
                            .tip("Regular expression  ⌥R")
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.toggle_dfind_regex(cx)
                            }))
                    }),
            )
            .child(
                super::hover::cut("find-count", count.clone())
                    .text_size(px(12.))
                    .text_color(
                        if count == "No matches" || self.review.read(cx).dfind_error.is_some() {
                            theme::removed()
                        } else {
                            theme::muted()
                        },
                    ),
            )
            .child(step("dfind-prev", "↑", false))
            .child(step("dfind-next", "↓", true))
            .child(
                chip("dfind-close", "✕", false)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_dfind(cx))),
            );
        Some(typing::input_box(bar, Input::DiffFind, cx))
    }

    /// Reads the paths of every loaded commit, a chunk at a time; the filter updates as they arrive.
    pub(super) fn index_paths(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let ids: Vec<Oid> = self
            .history
            .read(cx)
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
                let alive = this.update(cx, |this, cx| {
                    this.find.paths.extend(read);
                    if this.find.query.needs_paths() || this.find.text.is_some() {
                        this.refilter(cx);
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            this.update(cx, |this, cx| {
                this.find.indexing = false;
                cx.notify();
            })
            .ok();
        }));
    }

    /// The search field, its qualifier tags and the count line.
    pub(super) fn render_find(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let active = self.has_keyboard(Input::CommitSearch, cx);
        let text = self.find.text.clone().unwrap_or_default();
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
            .rounded(px(super::ROW_RADIUS))
            .border_2()
            .border_color(if active {
                theme::focus()
            } else {
                theme::island_border()
            })
            .bg(theme::base())
            .cursor_text()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_find(cx)))
            .child(
                super::icons::icon("search")
                    .size(px(14.))
                    .text_color(theme::faint()),
            )
            .child(self.field_text(&text, active, "Search commits", cx))
            .child(div().text_size(px(11.)).text_color(theme::faint()).child(
                super::keys::text(if text.is_empty() { "⌘⇧F" } else { "esc" }).into_owned(),
            ));
        let tags = active.then(|| {
            div()
                .flex_none()
                .flex()
                .gap_1()
                .px(px(8.))
                .pb(px(6.))
                .children(QUALIFIERS.into_iter().map(|q| {
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
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            let mut text = this.find.text.clone().unwrap_or_default();
                            if !text.is_empty() && !text.ends_with(' ') {
                                text.push(' ');
                            }
                            text.push_str(q);
                            this.set_find_text(text, cx);
                        }))
                }))
        });
        let count = self.find.shown.as_ref().map(|shown| {
            let mut line = format!(
                "{} of {} commits",
                shown.len(),
                self.history.read(cx).commits.len()
            );
            // Older history still being read for it (#25).
            if self.history.read(cx).older.is_some() {
                line.push_str(" · reading older ones…");
            } else if self.find.indexing {
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
        let el = div()
            .flex()
            .flex_col()
            .child(field)
            .children(tags)
            .children(count);
        typing::input_box(el, Input::CommitSearch, cx)
    }

    /// The summary of commit `ix`, with the search words highlighted.
    pub(super) fn summary_text(&self, summary: &SharedString) -> StyledText {
        let ranges = search::highlights(summary, self.find.query.words());
        StyledText::new(summary.clone()).with_highlights(ranges.into_iter().map(|range| {
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

    /// Shown instead of the list when nothing matches.
    pub(super) fn render_no_match(&self, cx: &App) -> impl IntoElement + use<> {
        let text = self.find.text.clone().unwrap_or_default();
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
            .child(div().text_size(px(11.)).text_color(theme::faint()).child(
                match self.history.read(cx).older {
                    Some(_) => format!(
                        "Searched the {} newest commits; reading older ones…",
                        self.history.read(cx).commits.len()
                    ),
                    None => format!(
                        "Searched all {} commits.",
                        self.history.read(cx).commits.len()
                    ),
                },
            ))
    }
}

use super::Selection;
