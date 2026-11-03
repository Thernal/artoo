//! The Inbox's model (#49, #72): which notifications it lists (`InboxFilter`), a row of it — a notice, or a note of
//! yours an agent answered (#116) —, what it keeps of the one selected (`InboxView`), and the code a notice's
//! discussion is on, read from the repository (`excerpt`). The tab draws it for now.

use crate::notices::{Kind, Notices};
use crate::requests::Requests;
use artoo_core::git::{DiffLine, DiffSettings, LineKind, Repo};
use artoo_requests_api::ReplyTo;
use artoo_ui_kit::excerpt::{Excerpt, ExcerptLine};
use gpui::{Context, Entity, Task};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Lines shown above and below the commented one.
const ABOVE: usize = 3;
const BELOW: usize = 2;

pub use artoo_requests_api::{Answers, InboxFilter, InboxRow};

/// Whether `filter` lists a notice of kind `kind`.
fn lets(filter: InboxFilter, kind: Kind) -> bool {
    match filter {
        InboxFilter::All => true,
        InboxFilter::Mentions => kind == Kind::Mention,
        InboxFilter::Agents => false,
        InboxFilter::Reviews => kind == Kind::Review,
    }
}

/// Whether `filter` lists the agents' answers.
fn lets_agents(filter: InboxFilter) -> bool {
    matches!(filter, InboxFilter::All | InboxFilter::Agents)
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
}

impl InboxView {
    /// What the Inbox lists under its filter, newest first: the `notices`, and the agents' `answers`.
    pub fn rows(&self, notices: &Notices, answers: Answers) -> Vec<InboxRow> {
        let filter = self.filter;
        let mut rows: Vec<(i64, InboxRow)> = notices
            .list
            .iter()
            .enumerate()
            .filter(|(_, n)| lets(filter, n.kind))
            .map(|(ix, n)| (n.created, InboxRow::Notice(ix)))
            .collect();
        if lets_agents(filter) {
            rows.extend(
                answers
                    .iter()
                    .map(|(id, at, _)| (*at, InboxRow::Agent(*id))),
            );
        }
        rows.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        rows.into_iter().map(|(_, row)| row).collect()
    }

    /// The selected row: an agent's answer while it waits for you, else the notice.
    pub fn current(&self, notices: &Notices, answers: Answers) -> Option<InboxRow> {
        match self.agent {
            Some(id) if answers.iter().any(|(i, ..)| *i == id) => Some(InboxRow::Agent(id)),
            _ => (!notices.list.is_empty()).then_some(InboxRow::Notice(notices.at)),
        }
    }

    /// j k: the row listed after the selected one, or before it; the first when none is.
    pub fn step(&self, notices: &Notices, answers: Answers, down: bool) -> Option<InboxRow> {
        let rows = self.rows(notices, answers);
        if rows.is_empty() {
            return None;
        }
        let at = self
            .current(notices, answers)
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
    if matches!(n.kind, Kind::Review | Kind::Todo) {
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
        Some(InboxRow::Agent(id)) => view.agent = Some(id),
        Some(InboxRow::Notice(ix)) => {
            view.agent = None;
            notices.at = ix;
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

    #[test]
    fn the_rows_are_newest_first_and_a_step_stops_at_the_ends() {
        let notice = |created: i64, kind: Kind| crate::notices::Notice {
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
        };
        let mut notices = Notices::default();
        notices.list = vec![notice(30, Kind::Review), notice(10, Kind::Mention)];
        let answers = [(7, 20, "agent:7:1".to_owned())];
        let mut view = InboxView::default();
        let rows = view.rows(&notices, &answers);
        assert_eq!(
            rows,
            [InboxRow::Notice(0), InboxRow::Agent(7), InboxRow::Notice(1)]
        );
        assert_eq!(view.current(&notices, &answers), Some(InboxRow::Notice(0)));
        assert_eq!(
            view.step(&notices, &answers, false),
            Some(InboxRow::Notice(0))
        );
        view.agent = Some(7);
        assert_eq!(
            view.step(&notices, &answers, true),
            Some(InboxRow::Notice(1))
        );
        // An answer no longer waiting is not selected.
        assert_eq!(view.current(&notices, &[]), Some(InboxRow::Notice(0)));
        view.filter = InboxFilter::Mentions;
        assert_eq!(view.rows(&notices, &answers), [InboxRow::Notice(1)]);
    }

    #[test]
    fn a_line_past_the_text_is_empty() {
        assert_eq!(nth("a\nb  \n", 2), "b");
        assert_eq!(nth("a\n", 3), "");
    }
}
