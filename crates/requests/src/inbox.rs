//! The Inbox's model (#49, #72): which notifications it lists (`InboxFilter`), a row of it — a notice, or a note of
//! yours an agent answered (#116) —, what it keeps of the one selected (`InboxView`), and the code a notice's
//! discussion is on, read from the repository (`excerpt`). The tab draws it for now.

use crate::notices::Notices;
use crate::requests::Requests;
use artoo_core::git::{DiffLine, DiffSettings, LineKind, Repo};
use artoo_requests_api::ReplyTo;
use artoo_ui_kit::excerpt::{Excerpt, ExcerptLine};
use gpui::{App, Context, Entity, Task};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Lines shown above and below the commented one.
const ABOVE: usize = 3;
const BELOW: usize = 2;

pub use artoo_requests_api::{Answers, InboxFilter, InboxRow};

use artoo_shell_api::notices::Why;
/// The parts' rows (`artoo_shell_api::notices::Listed`).
pub use artoo_shell_api::notices::{Listed, row_id};

/// A row the Inbox may list, with what its filters read of it (#197).
struct Item {
    row: InboxRow,
    at: i64,
    /// Its source's id: `requests`, `agents`, a plugin's.
    source: &'static str,
    why: Option<Why>,
    /// Not read yet.
    fresh: bool,
    /// What Filter looks in: what happened, what it is about, what was written.
    text: String,
}

/// Every row the Inbox may list: the `notices` of a kind whose Inbox is on (#196), the agents' `answers`, the parts'
/// rows; each under its source and what happened, as its kind was declared.
fn items(notices: &Notices, answers: Answers, parts: &[Listed], cx: &App) -> Vec<Item> {
    use artoo_agents_api::notices::{ASKS, DONE};
    let kind = |id: &str| artoo_shell_api::notices::kind(cx, id);
    let mut out: Vec<Item> = (notices.list.iter().enumerate())
        .filter(|(_, n)| n.listed(cx))
        .map(|(ix, n)| Item {
            row: InboxRow::Notice(ix),
            at: n.created,
            source: SOURCE,
            why: kind(n.kind.id()).and_then(|(k, _)| k.why),
            fresh: notices.unread_at(ix),
            text: format!(
                "{}\n{} {}\n{}",
                n.says(),
                artoo_ui_kit::words::number(n.iid),
                n.title,
                n.text
            ),
        })
        .collect();
    out.extend(answers.iter().map(|a| {
        let declared = kind(if a.asks { ASKS } else { DONE });
        Item {
            row: InboxRow::Agent(a.id),
            at: a.at,
            source: declared.as_ref().map_or("agents", |(_, s)| s.id),
            why: declared.and_then(|(k, _)| k.why),
            fresh: !notices.is_read(&a.key),
            text: a.text.clone(),
        }
    }));
    out.extend(parts.iter().filter_map(|r| {
        let (k, s) = kind(r.kind)?;
        Some(Item {
            row: InboxRow::Part(row_id(&r.key)),
            at: r.at,
            source: s.id,
            why: k.why,
            fresh: !notices.is_read(&r.key),
            text: format!("{}\n{}\n{}", r.says, r.about, r.text),
        })
    }));
    out
}

/// The requests' own source of notices (`crate::notices::source`).
pub const SOURCE: &str = artoo_requests_api::NOTICES;

/// What the Inbox counts as new (#197): under each of what happened and in all, within the source chosen; in each
/// source and in all of them, whatever happened.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct Counts {
    pub why: [usize; 4],
    pub all: usize,
    pub sources: Vec<(&'static str, usize)>,
    pub everywhere: usize,
}

impl Counts {
    pub fn of(&self, why: Why) -> usize {
        Why::ALL
            .iter()
            .position(|w| *w == why)
            .map_or(0, |at| self.why[at])
    }

    pub fn source(&self, id: &str) -> usize {
        (self.sources.iter())
            .find(|(s, _)| *s == id)
            .map_or(0, |(_, n)| *n)
    }
}

/// A source of the Inbox's rows, as Source ▾ lists it (#197).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRow {
    pub id: &'static str,
    pub name: String,
    /// Its rows not read yet.
    pub new: usize,
}

/// The sources whose kinds wait in the Inbox, in Settings' order, the requests' named by their `host` (`GitLab`).
pub fn sources(host: Option<&'static str>, counts: &Counts, cx: &App) -> Vec<SourceRow> {
    (artoo_shell_api::notices::sources(cx).into_iter())
        .filter(|s| s.kinds.iter().any(|k| k.inbox.is_some()))
        .map(|s| SourceRow {
            id: s.id,
            name: source_name(&s, host),
            new: counts.source(s.id),
        })
        .collect()
}

/// A source's name in the Inbox: the requests' is their host's.
pub fn source_name(
    source: &artoo_shell_api::notices::Source,
    host: Option<&'static str>,
) -> String {
    match host {
        Some(host) if source.id == SOURCE => host.to_owned(),
        _ => source.name.to_owned(),
    }
}

/// The rows of Source ▾ its field's `text` leaves: the sources with something new first, then the rest, their
/// names holding the text; with no text, All sources first (`None`).
pub fn picked_sources(sources: &[SourceRow], text: &str) -> Vec<Option<&'static str>> {
    let text = text.trim().to_lowercase();
    let hit = |s: &&SourceRow| text.is_empty() || s.name.to_lowercase().contains(&text);
    let all = text.is_empty().then_some(None);
    let live = sources.iter().filter(hit).filter(|s| s.new > 0);
    let quiet = sources.iter().filter(hit).filter(|s| s.new == 0);
    all.into_iter()
        .chain(live.chain(quiet).map(|s| Some(s.id)))
        .collect()
}

/// Source ▾'s field, one of the tab's inputs (`sources_input`).
pub const SOURCES: &str = "requests.inbox-sources";

/// Source ▾ open (#197): what is typed in its field, its caret, the row the keys point at, and its rows as last drawn
/// (`picked_sources`), for ↑ ↓ ↵.
#[derive(Default)]
pub struct SourcePick {
    pub text: String,
    pub area: artoo_ui_kit::input::Area,
    pub at: usize,
    pub rows: std::rc::Rc<std::cell::RefCell<Vec<Option<&'static str>>>>,
}

/// Source ▾'s field as the tab's input: typing narrows its list, ↑ ↓ move along it, ↵ picks — the first row of that
/// source selected, as a click does —, esc closes it; it closes as the keyboard leaves.
pub fn sources_input(
    view: Entity<InboxView>,
    services: std::rc::Weak<artoo_shell_api::Services>,
) -> artoo_shell_api::Input {
    use artoo_shell_api::{Input, Typed};
    let (open, key, left, selected) = (
        view.downgrade(),
        view.downgrade(),
        view.downgrade(),
        view.downgrade(),
    );
    Input {
        id: SOURCES,
        open: std::rc::Rc::new(move |cx| {
            open.upgrade().is_some_and(|v| v.read(cx).sources.is_some())
        }),
        key: std::rc::Rc::new(move |event, _, cx| {
            let Some(view) = key.upgrade() else {
                return Typed::Kept;
            };
            let picked = view.update(cx, |v, cx| {
                let shown = v.filter;
                let pick = v.sources.as_mut()?;
                let rows = pick.rows.borrow().clone();
                cx.notify();
                match event.keystroke.key.as_str() {
                    "down" => pick.at = (pick.at + 1).min(rows.len().saturating_sub(1)),
                    "up" => pick.at = pick.at.saturating_sub(1),
                    _ => match artoo_ui_kit::input::edit(
                        &mut pick.text,
                        &mut pick.area.edit,
                        &event.keystroke,
                        cx,
                    ) {
                        artoo_ui_kit::input::Edit::Changed => pick.at = 0,
                        artoo_ui_kit::input::Edit::Enter { .. } => {
                            let source = *rows.get(pick.at)?;
                            v.sources = None;
                            return Some(Some(InboxFilter { source, ..shown }));
                        }
                        artoo_ui_kit::input::Edit::Escape => {
                            v.sources = None;
                            return Some(None);
                        }
                        _ => {}
                    },
                }
                None
            });
            match picked {
                Some(Some(filter)) => {
                    // Picked: the tab selects its first row once its own update is over.
                    let host = (services.upgrade())
                        .and_then(|s| s.get::<std::rc::Rc<dyn artoo_requests_api::InboxHost>>());
                    if let Some(host) = host {
                        cx.defer(move |cx| {
                            host.act(artoo_requests_api::InboxEvent::Filter(filter), cx)
                        });
                    }
                    Typed::Leave
                }
                Some(None) => Typed::Leave,
                None => Typed::Kept,
            }
        }),
        left: std::rc::Rc::new(move |cx| {
            left.update(cx, |v, cx| {
                if v.sources.take().is_some() {
                    cx.notify();
                }
            })
            .ok();
        }),
        selected: std::rc::Rc::new(move |cx| {
            let v = selected.upgrade()?;
            let v = v.read(cx);
            v.sources
                .as_ref()?
                .area
                .edit
                .selected_text()
                .map(str::to_owned)
        }),
        caret: std::rc::Rc::new(|_| true),
    }
}

/// What the Inbox keeps of the notice selected and the code read for it.
#[derive(Default)]
pub struct InboxView {
    pub filter: InboxFilter,
    /// By discussion id; `None` once read and not found (the commit is not here).
    pub excerpts: HashMap<String, Option<Excerpt>>,
    /// The discussion whose code is being read: its excerpt shimmers.
    pub reading: Option<String>,
    pub task: Option<Task<()>>,
    /// What the composer writes to: request, and its discussion (none: a comment on the request).
    pub reply: Option<(u64, Option<String>)>,
    /// The agent's answer selected, by its comment's id; none: the notice at `notices.at`.
    pub agent: Option<u64>,
    /// A part's row selected, by its key's id (#196).
    pub part: Option<u64>,
    /// The row picked while it was new: it stays under New, read, until another is picked, so a click moves nothing.
    pub kept: Option<InboxRow>,
    /// Source ▾, while it is open, and where it was drawn: its list opens under it.
    pub sources: Option<SourcePick>,
    pub source_at: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    /// The tab's frame: Source ▾ takes the keyboard through it, and the filters' menu opens on it.
    pub frame: Option<std::rc::Weak<dyn artoo_shell_api::Frame>>,
}

impl InboxView {
    /// What the Inbox lists under its filters and the text in Filter (`query`): new first, then read, each newest
    /// first.
    pub fn rows(
        &self,
        notices: &Notices,
        answers: Answers,
        parts: &[Listed],
        query: &str,
        cx: &App,
    ) -> Vec<InboxRow> {
        let InboxFilter { why, source } = self.filter;
        let query = query.trim().to_lowercase();
        let mut rows: Vec<(bool, i64, InboxRow)> = items(notices, answers, parts, cx)
            .into_iter()
            .filter(|i| why.is_none_or(|w| i.why == Some(w)))
            .filter(|i| source.is_none_or(|s| i.source == s))
            .filter(|i| query.is_empty() || i.text.to_lowercase().contains(&query))
            .map(|i| (!(i.fresh || self.kept == Some(i.row)), i.at, i.row))
            .collect();
        rows.sort_by_key(|(read, at, _)| (*read, std::cmp::Reverse(*at)));
        rows.into_iter().map(|(_, _, row)| row).collect()
    }

    /// Whether `row` is listed under New: not read, or picked while it was.
    pub fn is_new(
        &self,
        row: InboxRow,
        notices: &Notices,
        answers: Answers,
        parts: &[Listed],
    ) -> bool {
        self.kept == Some(row)
            || match row {
                InboxRow::Notice(ix) => notices.unread_at(ix),
                InboxRow::Agent(id) => {
                    (answers.iter()).any(|a| a.id == id && !notices.is_read(&a.key))
                }
                InboxRow::Part(id) => {
                    (parts.iter()).any(|r| row_id(&r.key) == id && !notices.is_read(&r.key))
                }
            }
    }

    /// What is new, under each filter (`Counts`).
    pub fn counts(
        &self,
        notices: &Notices,
        answers: Answers,
        parts: &[Listed],
        cx: &App,
    ) -> Counts {
        let mut counts = Counts::default();
        for i in items(notices, answers, parts, cx)
            .into_iter()
            .filter(|i| i.fresh)
        {
            counts.everywhere += 1;
            match counts.sources.iter_mut().find(|(s, _)| *s == i.source) {
                Some((_, n)) => *n += 1,
                None => counts.sources.push((i.source, 1)),
            }
            if self.filter.source.is_some_and(|s| s != i.source) {
                continue;
            }
            counts.all += 1;
            if let Some(at) = i.why.and_then(|w| Why::ALL.iter().position(|o| *o == w)) {
                counts.why[at] += 1;
            }
        }
        counts
    }

    /// The selected row: an agent's answer while it waits for you, else the notice — the first listed when the one
    /// selected is of a kind no longer listed.
    pub fn current(
        &self,
        notices: &Notices,
        answers: Answers,
        parts: &[Listed],
        cx: &App,
    ) -> Option<InboxRow> {
        if let Some(id) = self
            .part
            .filter(|id| parts.iter().any(|r| row_id(&r.key) == *id))
        {
            return Some(InboxRow::Part(id));
        }
        match self.agent {
            Some(id) if answers.iter().any(|a| a.id == id) => Some(InboxRow::Agent(id)),
            _ => match notices.list.get(notices.at) {
                Some(n) if n.listed(cx) => Some(InboxRow::Notice(notices.at)),
                _ => (notices.list.iter().position(|n| n.listed(cx)))
                    .map(InboxRow::Notice)
                    .or_else(|| parts.first().map(|r| InboxRow::Part(row_id(&r.key)))),
            },
        }
    }

    /// j k: the row listed after the selected one, or before it; the first when none is.
    pub fn step(
        &self,
        notices: &Notices,
        answers: Answers,
        parts: &[Listed],
        query: &str,
        down: bool,
        cx: &App,
    ) -> Option<InboxRow> {
        let rows = self.rows(notices, answers, parts, query, cx);
        if rows.is_empty() {
            return None;
        }
        let at = self
            .current(notices, answers, parts, cx)
            .and_then(|r| rows.iter().position(|&o| o == r));
        let next = match (at, down) {
            (Some(i), true) => (i + 1).min(rows.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
            (None, _) => 0,
        };
        Some(rows[next])
    }
}

impl Requests {
    /// The notice selected changed (or its discussions came in): reads the code its discussion is on into `view`,
    /// once, in the background.
    pub(crate) fn read_excerpt(
        &mut self,
        notices: &Entity<Notices>,
        view: &Entity<InboxView>,
        cx: &mut Context<Self>,
    ) {
        let ns = notices.read(cx);
        let Some(n) = ns.list.get(ns.at) else {
            return;
        };
        let Some(t) = n.note.and_then(|note| ns.thread_of(n.iid, note)) else {
            return;
        };
        if view.read(cx).excerpts.contains_key(&t.id) {
            return;
        }
        let (Some(root), Some(path)) = (self.root.clone(), t.path.clone()) else {
            return;
        };
        let Some(m) = self.find(n.iid) else {
            return;
        };
        let heads: Vec<String> = t.commit.iter().cloned().chain([m.head.clone()]).collect();
        let (id, target, new, old) = (t.id.clone(), m.target.clone(), t.new_line, t.old_line);
        let (reading, into) = (id.clone(), view.downgrade());
        let task = cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { excerpt(&root, &target, &heads, &path, new, old) })
                .await;
            into.update(cx, |v, _| {
                v.reading.take_if(|r| *r == id);
                v.excerpts.insert(id, found);
            })
            .ok();
            this.update(cx, |_, cx| cx.notify()).ok();
        });
        view.update(cx, |v, _| {
            v.reading = Some(reading);
            v.task = Some(task);
        });
    }
}

/// What the notice selected in `notices` is answered with; none for a review request or a to-do.
pub(crate) fn reply_to(notices: &Notices) -> Option<ReplyTo> {
    let n = notices.list.get(notices.at)?;
    if !n.kind.answerable() {
        return None;
    }
    let thread = n.note.and_then(|note| notices.thread_of(n.iid, note));
    Some(match thread {
        Some(t) => (
            n.iid,
            Some(t.id.clone()),
            t.path.clone().zip(t.new_line.or(t.old_line)),
        ),
        None => (n.iid, None, None),
    })
}

/// Row `row` of the Inbox selected in `view` and `notices` (none: the first notice).
pub(crate) fn pick(row: Option<InboxRow>, view: &mut InboxView, notices: &mut Notices) {
    match row {
        Some(InboxRow::Agent(id)) => {
            view.part = None;
            view.agent = Some(id);
        }
        Some(InboxRow::Notice(ix)) => {
            view.agent = None;
            view.part = None;
            notices.at = ix;
        }
        Some(InboxRow::Part(id)) => {
            view.agent = None;
            view.part = Some(id);
        }
        None => notices.at = 0,
    }
}

/// The lines around line `new` (or `old`, a removed line) of `path`, as the request changes them:
/// from where its head (the first of `heads` that is here) left `origin/<target>`. A line the
/// request did not change shows as unchanged code at the head.
pub fn excerpt(
    root: &Path,
    target: &str,
    heads: &[String],
    path: &str,
    new: Option<u32>,
    old: Option<u32>,
) -> Option<Excerpt> {
    let repo = Repo::open(&PathBuf::from(root)).ok()?;
    let head = heads.iter().find_map(|h| repo.resolve(h).ok())?;
    let line = new.or(old)?;
    let base = repo
        .resolve(&format!("origin/{target}"))
        .or_else(|_| repo.resolve(target))
        .ok();
    let file = base
        .and_then(|base| repo.compare(base, head, true, DiffSettings::default()).ok())
        .and_then(|c| c.files.into_iter().find(|f| f.path() == path));
    if let Some(file) = file {
        for hunk in &file.hunks {
            if let Some(at) = position(&hunk.lines, new, old) {
                let (from, to) = around(hunk.lines.len(), at);
                let old_text = file.old_text.as_deref().unwrap_or_default();
                let new_text = file.new_text.as_deref().unwrap_or_default();
                let lines = hunk.lines[from..to]
                    .iter()
                    .enumerate()
                    .map(|(i, l)| ExcerptLine {
                        kind: l.kind,
                        old: l.old_line,
                        new: l.new_line,
                        text: match (l.kind, l.old_line, l.new_line) {
                            (LineKind::Removed, Some(o), _) => nth(old_text, o),
                            (_, _, Some(n)) => nth(new_text, n),
                            _ => String::new(),
                        },
                        on: from + i == at,
                    })
                    .collect();
                return Some(Excerpt { lines });
            }
        }
    }
    // Not among the changes: the code as it is at the head.
    let text = repo.file_at(head, path)?;
    let count = text.lines().count();
    let at = (line as usize).checked_sub(1).filter(|&at| at < count)?;
    let (from, to) = around(count, at);
    Some(Excerpt {
        lines: (from..to)
            .map(|i| ExcerptLine {
                kind: LineKind::Context,
                old: None,
                new: Some(i as u32 + 1),
                text: nth(&text, i as u32 + 1),
                on: i == at,
            })
            .collect(),
    })
}

/// Where in a hunk the discussion's line is: by its new number, or a removed line by its old.
fn position(lines: &[DiffLine], new: Option<u32>, old: Option<u32>) -> Option<usize> {
    match new {
        Some(n) => lines.iter().position(|l| l.new_line == Some(n)),
        None => lines
            .iter()
            .position(|l| l.kind == LineKind::Removed && l.old_line == old),
    }
}

/// The rows shown of `len` around row `at`: `ABOVE` before it and `BELOW` after.
fn around(len: usize, at: usize) -> (usize, usize) {
    (at.saturating_sub(ABOVE), (at + BELOW + 1).min(len))
}

/// Line `n` (1-based) of `text`, without its end.
fn nth(text: &str, n: u32) -> String {
    text.lines()
        .nth(n as usize - 1)
        .map(|l| l.trim_end().to_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>) -> DiffLine {
        DiffLine {
            kind,
            old_line: old,
            new_line: new,
        }
    }

    #[test]
    fn the_rows_around_a_line_stop_at_the_edges() {
        assert_eq!(around(10, 5), (2, 8));
        assert_eq!(around(10, 1), (0, 4));
        assert_eq!(around(4, 3), (0, 4));
    }

    #[test]
    fn a_removed_line_is_found_by_its_old_number() {
        let lines = [
            line(LineKind::Context, Some(4), Some(4)),
            line(LineKind::Removed, Some(5), None),
            line(LineKind::Added, None, Some(5)),
        ];
        assert_eq!(position(&lines, None, Some(5)), Some(1));
        assert_eq!(position(&lines, Some(5), None), Some(2));
        assert_eq!(position(&lines, Some(4), Some(4)), Some(0));
        assert_eq!(position(&lines, Some(9), None), None);
    }

    use crate::notices::{Kind, Notice};
    use artoo_core::notice::Answer;

    fn notice(created: i64, kind: Kind) -> Notice {
        Notice {
            key: format!("todo:{created}"),
            kind,
            iid: 1,
            title: String::new(),
            who: String::new(),
            text: String::new(),
            created,
            note: None,
            todo: None,
            resolved: None,
        }
    }

    fn answer(id: u64, at: i64, asks: bool, text: &str) -> Answer {
        Answer {
            id,
            at,
            key: format!("agent:{id}:1"),
            asks,
            text: text.into(),
        }
    }

    #[test]
    fn the_rows_are_newest_first_and_a_step_stops_at_the_ends() {
        let mut notices = Notices::default();
        notices.list = vec![notice(30, Kind::Review), notice(10, Kind::Mention)];
        let answers = [answer(7, 20, true, "")];
        let mut view = InboxView::default();
        gpui::TestAppContext::single().update(|cx| {
            artoo_shell_api::notices::add_source(cx, crate::notices::source());
            let rows = view.rows(&notices, &answers, &[], "", cx);
            assert_eq!(
                rows,
                [InboxRow::Notice(0), InboxRow::Agent(7), InboxRow::Notice(1)]
            );
            assert_eq!(
                view.current(&notices, &answers, &[], cx),
                Some(InboxRow::Notice(0))
            );
            assert_eq!(
                view.step(&notices, &answers, &[], "", false, cx),
                Some(InboxRow::Notice(0))
            );
            view.agent = Some(7);
            assert_eq!(
                view.step(&notices, &answers, &[], "", true, cx),
                Some(InboxRow::Notice(1))
            );
            // An answer no longer waiting is not selected.
            assert_eq!(
                view.current(&notices, &[], &[], cx),
                Some(InboxRow::Notice(0))
            );
            view.filter.why = Some(Why::Mentions);
            assert_eq!(
                view.rows(&notices, &answers, &[], "", cx),
                [InboxRow::Notice(1)]
            );
            // Review requests turned off in the Inbox (#196): neither listed nor selected.
            view.filter = InboxFilter::default();
            view.agent = None;
            let off = artoo_shell_api::notices::Channel::Inbox;
            artoo_shell_api::notices::set(cx, "requests.review", off, false);
            assert_eq!(
                view.rows(&notices, &answers, &[], "", cx),
                [InboxRow::Agent(7), InboxRow::Notice(1)]
            );
            assert_eq!(
                view.current(&notices, &answers, &[], cx),
                Some(InboxRow::Notice(1))
            );
            assert_eq!(notices.unread(cx), 1);
        });
    }

    #[test]
    fn new_rows_come_first_and_the_filters_count_what_is_new() {
        let mut notices = Notices::default();
        notices.list = vec![
            notice(50, Kind::Mention),
            notice(40, Kind::Failed),
            notice(30, Kind::Review),
            notice(20, Kind::Failed),
        ];
        // The newest is read: it goes under Read, below every new row.
        notices.mark_read("todo:50".into());
        let answers = [answer(7, 35, true, "Which rounding mode?")];
        let mut view = InboxView::default();
        gpui::TestAppContext::single().update(|cx| {
            artoo_shell_api::notices::add_source(cx, crate::notices::source());
            assert_eq!(
                view.rows(&notices, &answers, &[], "", cx),
                [
                    InboxRow::Notice(1),
                    InboxRow::Agent(7),
                    InboxRow::Notice(2),
                    InboxRow::Notice(3),
                    InboxRow::Notice(0),
                ]
            );
            let counts = view.counts(&notices, &answers, &[], cx);
            assert_eq!(counts.all, 4);
            assert_eq!(counts.of(Why::Failures), 2);
            assert_eq!(counts.of(Why::Reviews), 1);
            assert_eq!(counts.of(Why::Mentions), 0);
            assert_eq!(counts.source(SOURCE), 3);
            assert_eq!(counts.source("agents"), 1);
            assert_eq!(counts.everywhere, 4);
            // A source chosen: the kinds count within it, the sources still each their own.
            view.filter.source = Some("agents");
            let counts = view.counts(&notices, &answers, &[], cx);
            assert_eq!((counts.all, counts.of(Why::Failures)), (1, 0));
            assert_eq!((counts.source(SOURCE), counts.everywhere), (3, 4));
            assert_eq!(
                view.rows(&notices, &answers, &[], "", cx),
                [InboxRow::Agent(7)]
            );
            // Picked while new, a row stays under New once read.
            view.filter = InboxFilter::default();
            notices.mark_read("todo:40".into());
            view.kept = Some(InboxRow::Notice(1));
            assert_eq!(
                view.rows(&notices, &answers, &[], "", cx)[0],
                InboxRow::Notice(1)
            );
            assert!(view.is_new(InboxRow::Notice(1), &notices, &answers, &[]));
        });
    }

    #[test]
    fn filter_looks_in_what_happened_and_what_was_written() {
        let mut notices = Notices::default();
        let mut refund = notice(30, Kind::Mention);
        refund.who = "Kestrel Okafor".into();
        refund.title = "Refund rounds down".into();
        notices.list = vec![refund, notice(20, Kind::Failed)];
        let answers = [answer(7, 10, false, "Rounded half-even now")];
        let view = InboxView::default();
        gpui::TestAppContext::single().update(|cx| {
            artoo_shell_api::notices::add_source(cx, crate::notices::source());
            let rows = |q: &str| view.rows(&notices, &answers, &[], q, cx);
            assert_eq!(rows("kestrel"), [InboxRow::Notice(0)]);
            assert_eq!(rows(" REFUND "), [InboxRow::Notice(0)]);
            assert_eq!(rows("pipeline failed"), [InboxRow::Notice(1)]);
            assert_eq!(rows("half-even"), [InboxRow::Agent(7)]);
            assert!(rows("nothing like it").is_empty());
        });
    }

    #[test]
    fn source_lists_what_has_news_first_and_narrows_by_name() {
        let row = |id, name: &str, new| SourceRow {
            id,
            name: name.into(),
            new,
        };
        let sources = [
            row("requests", "GitLab", 2),
            row("sentry", "Sentry", 0),
            row("agents", "Agents", 1),
        ];
        assert_eq!(
            picked_sources(&sources, ""),
            [None, Some("requests"), Some("agents"), Some("sentry")]
        );
        assert_eq!(
            picked_sources(&sources, "en"),
            [Some("agents"), Some("sentry")]
        );
        assert!(picked_sources(&sources, "jira").is_empty());
    }

    #[test]
    fn a_line_past_the_text_is_empty() {
        assert_eq!(nth("a\nb  \n", 2), "b");
        assert_eq!(nth("a\n", 3), "");
    }
}
