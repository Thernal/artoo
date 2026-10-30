//! Discussions written on an earlier version (Design #46 B, `docs/specs/outdated-discussions.md`):
//! a thread keeps the commit it was written on, and the line it was written on is followed version
//! by version to the latest (`git::Repo::follow_line`) — who changed it, in which commit, what it
//! says now. The latest diff draws such a thread at its line now, saying Changed or Unchanged since
//! the comment — "your comment" only when it is yours, else its author is named — with the line's
//! history, and Compare v1 → v3; a thread whose line is gone stays only in the Discussions window,
//! grouped by the version it was written on.

use super::{Selection, Workspace, px, theme};
use crate::worddiff::Piece;
pub(super) use artoo_requests::outdated::{LineHistory, Since};
use gpui::{
    AnyElement, App, Context, FontWeight, HighlightStyle, StrikethroughStyle, StyledText,
    WeakEntity, div, prelude::*,
};

impl Workspace {
    /// Follows the line of every discussion written on an earlier version to the latest
    /// (`Requests::compute_histories`).
    pub(super) fn compute_histories(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.compute_histories(root, cx));
    }

    /// The button that compares thread `ix`'s version with the latest: "Compare v1 → v3".
    pub(super) fn compare_label(&self, ix: usize, cx: &App) -> Option<String> {
        let t = self.requests.read(cx).threads.get(ix)?;
        let written = self.requests.read(cx).histories.get(&t.id)?.written;
        let latest = self.versions.last()?.number;
        Some(format!("Compare v{written} → v{latest}"))
    }

    /// Under an outdated discussion: the line's history and Compare with the new version.
    pub(super) fn render_history(
        &self,
        ix: usize,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> Option<AnyElement> {
        let t = self.requests.read(cx).threads.get(ix)?;
        let h = self.requests.read(cx).histories.get(&t.id)?.clone();
        let changed = h.changed();
        let row = |tag: String, who: String, text: Option<AnyElement>, struck: bool| {
            div()
                .flex()
                .items_start()
                .gap(px(10.))
                .child(
                    div()
                        .w(px(28.))
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::accent())
                        .child(tag),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_color(theme::muted()).child(who))
                        .children(text.map(|t| {
                            div()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(4.))
                                .bg(if struck {
                                    theme::removed_line()
                                } else {
                                    theme::added_line()
                                })
                                .font_family(theme::code_font())
                                .text_size(px(12.))
                                .text_color(theme::text())
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(t)
                        })),
                )
        };
        // Who wrote the comment, named unless it is you.
        let who = if self.requests.read(cx).yours(t) {
            "you commented on".to_owned()
        } else {
            let author = t.notes.first().map_or("", |n| n.author.as_str());
            format!("{author} commented on")
        };
        let compare = self.compare_label(ix, cx).unwrap_or_default();
        let mut rows: Vec<AnyElement> = vec![
            row(
                format!("v{}", h.written),
                who,
                Some(h.then.clone().into_any_element()),
                changed,
            )
            .into_any_element(),
        ];
        // Each version's line against the one before it, the changed words marked (#46 B).
        let mut before = h.then.clone();
        for s in &h.steps {
            let text = match &s.now {
                Some(now) => {
                    let marked = word_line(&before, now);
                    before = now.clone();
                    marked
                }
                None => "(removed)".into_any_element(),
            };
            rows.push(
                row(
                    format!("v{}", s.version),
                    if s.summary.is_empty() {
                        "changed".to_owned()
                    } else {
                        format!("{} · {} {}", s.who, s.sha, s.summary)
                    },
                    Some(text),
                    s.now.is_none(),
                )
                .into_any_element(),
            );
        }
        let written = h.written;
        Some(
            div()
                .mx(px(12.))
                .mb(px(8.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(8.))
                .bg(theme::base())
                .flex()
                .flex_col()
                .gap(px(8.))
                .text_size(px(12.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .px(px(8.))
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .bg(theme::warning_bg())
                                .text_color(theme::warning())
                                .child(
                                    self.requests.read(cx).since_comment(ix).unwrap_or_default(),
                                ),
                        )
                        .child(
                            div()
                                .text_color(theme::faint())
                                .child(format!("written on v{written}")),
                        )
                        .child(div().flex_1())
                        .children(changed.then(|| {
                            div()
                                .id(("compare-since", ix))
                                .px(px(8.))
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .rounded(px(6.))
                                .border_1()
                                .border_color(theme::island_border())
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::hover()))
                                .flex_none()
                                .child(compare)
                                .on_click(move |_, _, cx| {
                                    this.update(cx, |this, cx| this.compare_since(ix, cx)).ok();
                                })
                        })),
                )
                .children(rows)
                .into_any_element(),
        )
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
        let Some(latest) = self.versions.last().map(|v| v.number) else {
            return;
        };
        let stops = h.stops(t.new_line.or(t.old_line), latest);
        let last = stops.len() - 1;
        let thread = t.id.clone();
        self.since = Some(Since {
            iid,
            thread,
            path,
            stops,
        });
        self.go_since(0, last, cx);
    }

    /// The stepper: versions `a` → `b` of the stops, at the line in `b`.
    fn go_since(&mut self, a: usize, b: usize, cx: &mut Context<Self>) {
        let Some(since) = self.since.clone() else {
            return;
        };
        let index = |number: usize| self.versions.iter().position(|v| v.number == number);
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
        self.version_tab = super::VersionTab::Files;
        self.load_versions(from, to, Some(since.path.clone()), cx);
    }

    /// The stepper's comparison of file `path` shown: its discussion pinned on the line, in the
    /// later version or, gone there, in the earlier one (#46 C).
    pub(super) fn since_thread_in(&self, path: &str, cx: &App) -> Vec<(usize, bool, u32)> {
        let Some(since) = self.since.as_ref().filter(|s| s.path == path) else {
            return Vec::new();
        };
        let Selection::Versions { from, to } = self.selection else {
            return Vec::new();
        };
        let number = |i: usize| self.versions.get(i).map(|v| v.number);
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

    /// Over the diff while it is one of the stepper's comparisons: the versions that changed the
    /// comment's line, and the way back to all changes.
    pub(super) fn render_since_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let since = self.since.as_ref()?;
        let Selection::Versions { from, to } = self.selection else {
            return None;
        };
        let shown = (
            self.versions.get(from)?.number,
            self.versions.get(to)?.number,
        );
        let last = since.stops.len() - 1;
        let mut pairs = vec![(0, last)];
        if last > 1 {
            pairs.extend((0..last).map(|i| (i, i + 1)));
        }
        let number = |i: usize| since.stops[i].0;
        if !pairs.iter().any(|&(a, b)| (number(a), number(b)) == shown) {
            return None;
        }
        let iid = since.iid;
        let yours = self
            .requests
            .read(cx)
            .threads
            .iter()
            .find(|t| t.id == since.thread)
            .is_some_and(|t| self.requests.read(cx).yours(t));
        let (line, whose) = if yours {
            ("Your comment's line", "your comment")
        } else {
            ("The commented line", "the comment")
        };
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap_2()
                .px(px(12.))
                .py(px(6.))
                .border_b_1()
                .border_color(theme::island_border())
                .text_size(px(12.))
                .child(div().text_color(theme::warning()).child(line))
                .child(
                    super::diff_view::group().children(pairs.into_iter().map(|(a, b)| {
                        let label = if (a, b) == (0, last) {
                            format!("v{} → v{} · {whose}", number(a), number(b))
                        } else {
                            format!("v{} → v{}", number(a), number(b))
                        };
                        let on = (number(a), number(b)) == shown;
                        super::chip(("since-step", a * 100 + b), label, on).on_click(cx.listener(
                            move |this, _: &gpui::ClickEvent, _, cx| this.go_since(a, b, cx),
                        ))
                    })),
                )
                .child(div().flex_1())
                .child(
                    super::chip("since-all", "All changes", false).on_click(cx.listener(
                        move |this, _: &gpui::ClickEvent, _, cx| {
                            this.since = None;
                            this.select_request(iid, cx);
                        },
                    )),
                )
                .into_any_element(),
        )
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

/// `new` with what changed from `old` marked: removed words struck, added ones tinted, both in
/// place; a line rewritten past recognition reads whole.
fn word_line(old: &str, new: &str) -> AnyElement {
    let Some(pieces) = crate::worddiff::pieces(old, new) else {
        return new.to_owned().into_any_element();
    };
    let (text, runs) = joined(old, new, &pieces);
    let style = |gone: bool| HighlightStyle {
        background_color: Some(if gone {
            theme::removed_word().into()
        } else {
            theme::added_word().into()
        }),
        strikethrough: gone.then_some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(theme::removed().into()),
        }),
        ..Default::default()
    };
    StyledText::new(text)
        .with_highlights(runs.into_iter().map(move |(r, gone)| (r, style(gone))))
        .into_any_element()
}

/// One text from `old` and `new` read together: what stayed, then for each change what went and
/// what came. The ranges of what went (`true`) and what came (`false`) in it.
fn joined(old: &str, new: &str, pieces: &[Piece]) -> (String, Vec<(std::ops::Range<usize>, bool)>) {
    let mut text = String::new();
    let mut runs = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Same(_, n) => text.push_str(&new[n.clone()]),
            Piece::Change(o, n) => {
                for (part, gone) in [(&old[o.clone()], true), (&new[n.clone()], false)] {
                    if !part.is_empty() {
                        let at = text.len();
                        text.push_str(part);
                        runs.push((at..text.len(), gone));
                    }
                }
            }
        }
    }
    (text, runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_requests::outdated::Step;

    fn step(version: usize, line: Option<u32>) -> Step {
        Step {
            version,
            who: String::new(),
            summary: String::new(),
            sha: String::new(),
            now: line.map(|_| String::new()),
            line,
        }
    }

    #[test]
    fn the_stepper_stops_where_the_line_changed_and_at_the_latest() {
        let h = LineHistory {
            written: 3,
            then: String::new(),
            steps: vec![step(5, Some(50)), step(6, Some(52))],
            line_now: Some(55),
        };
        assert_eq!(
            h.stops(Some(48), 7),
            [(3, Some(48)), (5, Some(50)), (6, Some(52)), (7, Some(55))]
        );
        // Changed in the latest itself: no extra stop.
        assert_eq!(h.stops(Some(48), 6).len(), 3);
    }

    #[test]
    fn a_changed_line_reads_once_with_both_sides_of_each_change() {
        let (old, new) = (
            "val result = api.login(form)",
            "val result = runCatching { api.login(form) }",
        );
        let pieces = crate::worddiff::pieces(old, new).expect("alike lines");
        let (text, runs) = joined(old, new, &pieces);
        assert!(text.contains("runCatching"));
        assert!(runs.iter().all(|(_, gone)| !gone));
        let added: Vec<&str> = runs.iter().map(|(r, _)| &text[r.clone()]).collect();
        assert!(added.iter().any(|a| a.contains("runCatching")));
    }
}
