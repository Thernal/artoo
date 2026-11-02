//! Discussions written on an earlier version (Design #46 B, `docs/specs/outdated-discussions.md`):
//! a thread keeps the commit it was written on, and the line it was written on is followed version
//! by version to the latest (`git::Repo::follow_line`) — who changed it, in which commit, what it
//! says now. The latest diff draws such a thread at its line now, saying Changed or Unchanged since
//! the comment — "your comment" only when it is yours, else its author is named — with the line's
//! history, and Compare v1 → v3; a thread whose line is gone stays only in the Discussions window,
//! grouped by the version it was written on.

use super::{Selection, Workspace};
pub(super) use artoo_requests::outdated::{LineHistory, Since};
use gpui::{App, Context};

impl Workspace {
    /// The timeline's last version: what an outdated discussion's is compared with.
    pub(super) fn latest_version(&self, cx: &App) -> Option<usize> {
        self.branches().and_then(|b| b.latest_version(cx)).map(|v| v.number)
    }

    /// Follows the line of every discussion written on an earlier version to the latest
    /// (`Requests::compute_histories`).
    pub(super) fn compute_histories(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.compute_histories(root, cx));
    }

    /// Compare with the new version, for discussion `ix`: its version against the latest, at its
    /// line now with the whole function around it (or, the line gone, where it was), and a stepper
    /// through the versions that changed it.
    pub(super) fn compare_since(&mut self, ix: usize, cx: &mut Context<Self>) {
        let (Some(t), Some(iid)) = (
            self.requests.read(cx).threads.get(ix),
            self.requests.read(cx).current,
        ) else {
            return;
        };
        let (Some(h), Some(path)) = (self.requests.read(cx).histories.get(&t.id), t.path.clone())
        else {
            return;
        };
        let Some(latest) = self.branches().and_then(|b| b.latest_version(cx)).map(|v| v.number) else {
            return;
        };
        let stops = h.stops(t.new_line.or(t.old_line), latest);
        let last = stops.len() - 1;
        let thread = t.id.clone();
        self.requests.update(cx, |rq, _| {
            rq.since = Some(Since {
                iid,
                thread,
                path,
                stops,
            })
        });
        self.go_since(0, last, cx);
    }

    /// The stepper: versions `a` → `b` of the stops, at the line in `b`.
    pub(super) fn go_since(&mut self, a: usize, b: usize, cx: &mut Context<Self>) {
        let Some(since) = self.requests.read(cx).since.clone() else {
            return;
        };
        let index = |number: usize| {
            (self.branches()?.versions(cx))
                .iter()
                .position(|v| v.0 == number)
        };
        let (Some(&(va, la)), Some(&(vb, lb))) = (since.stops.get(a), since.stops.get(b)) else {
            return;
        };
        let (Some(from), Some(to)) = (index(va), index(vb)) else {
            return;
        };
        // The comment's file once the comparison is in, at its line: where it is in `b`, or gone
        // there, where it was in `a` on the old side; the whole function around it.
        let at = lb.map(|l| (l, false)).or(la.map(|l| (l, true)));
        self.review.update(cx, |rv, _| {
            rv.jump_line = at;
            rv.jump_whole = at.is_some();
        });
        if let Some(branches) = self.branches() {
            branches.show_version_files(cx);
        }
        self.load_versions(from, to, Some(since.path.clone()), cx);
    }

    /// The stepper's comparison of file `path` shown: its discussion pinned on the line, in the
    /// later version or, gone there, in the earlier one (#46 C).
    pub(super) fn since_thread_in(&self, path: &str, cx: &App) -> Vec<(usize, bool, u32)> {
        let Some(since) = self
            .requests
            .read(cx)
            .since
            .as_ref()
            .filter(|s| s.path == path)
        else {
            return Vec::new();
        };
        let Selection::Versions { from, to } = self.selection(cx) else {
            return Vec::new();
        };
        let number = |i: usize| self.version_at(i, cx).map(|v| v.number);
        let stop = |n: Option<usize>| since.stops.iter().find(|(v, _)| Some(*v) == n);
        let (Some(&(_, a)), Some(&(_, b))) = (stop(number(from)), stop(number(to))) else {
            return Vec::new();
        };
        let ix = self
            .requests
            .read(cx)
            .threads
            .iter()
            .position(|t| t.id == since.thread);
        let at = b.map(|l| (false, l)).or(a.map(|l| (true, l)));
        ix.zip(at)
            .map(|(ix, (old, line))| vec![(ix, old, line)])
            .unwrap_or_default()
    }

    /// The stepper's comparison left: all the request's changes again.
    pub(super) fn leave_since(&mut self, cx: &mut Context<Self>) {
        if let Some(since) = self.requests.update(cx, |rq, _| rq.since.take()) {
            self.open_request(since.iid, cx);
        }
    }

    /// Shows the unchanged lines of the function around `line` (the side `old`), so a comparison
    /// reads the whole function (#46 C).
    pub(super) fn reveal_function(&mut self, line: u32, old: bool, cx: &mut App) {
        let Some(data) = self.review.read(cx).data.clone() else {
            return;
        };
        let side = data.side(old);
        let Some((start, end)) = crate::symbols::enclosing(side.count(), line, |n| side.line(n).0)
        else {
            return;
        };
        let mut grew = false;
        for (ix, segment) in data.segments.iter().enumerate() {
            if let super::rows::Segment::Same {
                old: o,
                new: n,
                len,
            } = *segment
            {
                let first = if old { o } else { n };
                if first <= end && first + len > start {
                    grew |= self.review.update(cx, |rv, _| rv.expanded.insert(ix));
                }
            }
        }
        if grew {
            self.relayout(cx);
        }
    }

    /// A notification opened on comment `note`: once the histories are in, an outdated one whose
    /// line changed opens on the diff since it (#46, the inbox way in).
    pub(super) fn open_since_on_arrival(&mut self, cx: &mut Context<Self>) {
        let Some((iid, note)) = self.since_on_open else {
            return;
        };
        if self.requests.read(cx).current != Some(iid) {
            return;
        }
        self.since_on_open = None;
        let ix = self
            .requests
            .read(cx)
            .threads
            .iter()
            .position(|t| t.notes.iter().any(|n| n.id == note));
        if let Some(ix) = ix
            && self
                .requests
                .read(cx)
                .histories
                .get(&self.requests.read(cx).threads[ix].id)
                .is_some_and(LineHistory::changed)
        {
            self.compare_since(ix, cx);
        }
    }
}
