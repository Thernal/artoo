//! What became of the line an outdated discussion was written on, version by version (#46), followed in the
//! background, and the comparison opened from one to step through them, its bar over the diff (`since_piece`).

use crate::requests::{Act, Acts, Event, Requests};
use crate::thread::GUTTER;
use artoo_core::git::{Fate, Repo};
use artoo_core::route::Place;
use artoo_core::worddiff::{self, Piece};
use artoo_hosts_api::Thread;
use artoo_requests_api::RequestVersionsRoute;
use artoo_review_api::Review;
use artoo_review_api::diff::Header;
use artoo_shell_api::{Services, Slot};
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::hover::Hover as _;
use artoo_ui_kit::{discussion, hover, px, theme};
use gpui::{
    AnyElement, ClickEvent, Context, Entity, HighlightStyle, SharedString, StyledText, div,
    prelude::*,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

/// One later version that changed the line.
#[derive(Clone, Debug)]
pub struct Step {
    pub version: usize,
    pub who: String,
    pub summary: String,
    pub sha: String,
    /// What the line says in that version; None when it was removed.
    pub now: Option<String>,
    /// Where the line is in that version.
    pub line: Option<u32>,
}

/// What became of the line a discussion was written on.
#[derive(Clone, Debug)]
pub struct LineHistory {
    /// The version it was written on.
    pub written: usize,
    /// The line as it read then.
    pub then: String,
    pub steps: Vec<Step>,
    /// Where the line is in the latest version; None when it is gone.
    pub line_now: Option<u32>,
}

impl LineHistory {
    pub fn changed(&self) -> bool {
        !self.steps.is_empty()
    }

    /// The versions to step through: the one it was written on (at `line` then), each that
    /// changed the line, and the latest (`latest`) when the line did not change in it.
    pub fn stops(&self, line: Option<u32>, latest: usize) -> Vec<(usize, Option<u32>)> {
        let mut stops = vec![(self.written, line)];
        stops.extend(self.steps.iter().map(|s| (s.version, s.line)));
        if stops.last().is_some_and(|(v, _)| *v < latest) {
            stops.push((latest, self.line_now));
        }
        stops
    }
}

pub use artoo_requests_api::Since;

impl Requests {
    /// Over the diff while it is one of the stepper's comparisons (`shown`, the versions compared): the versions
    /// that changed the comment's line, and the way back to all changes.
    pub fn since_bar(&self, shown: (usize, usize), acts: &Acts) -> Option<AnyElement> {
        let since = self.since.as_ref()?;
        let last = since.stops.len() - 1;
        let mut pairs = vec![(0, last)];
        if last > 1 {
            pairs.extend((0..last).map(|i| (i, i + 1)));
        }
        let number = |i: usize| since.stops[i].0;
        if !pairs.iter().any(|&(a, b)| (number(a), number(b)) == shown) {
            return None;
        }
        let yours = self
            .threads
            .iter()
            .find(|t| t.id == since.thread)
            .is_some_and(|t| self.yours(t));
        let (line, whose) = if yours {
            ("Your comment's line", "your comment")
        } else {
            ("The commented line", "the comment")
        };
        let all = acts.clone();
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
                .child(group().children(pairs.into_iter().map(|(a, b)| {
                    let label = if (a, b) == (0, last) {
                        format!("v{} → v{} · {whose}", number(a), number(b))
                    } else {
                        format!("v{} → v{}", number(a), number(b))
                    };
                    let on = (number(a), number(b)) == shown;
                    let acts = acts.clone();
                    chip(("since-step", a * 100 + b), label, on)
                        .on_click(move |_: &ClickEvent, _, cx| acts(Act::SinceStep { a, b }, cx))
                })))
                .child(div().flex_1())
                .child(
                    chip("since-all", "All changes", false)
                        .on_click(move |_: &ClickEvent, _, cx| all(Act::SinceAll, cx)),
                )
                .into_any_element(),
        )
    }
}

/// The stepper's bar at the top of the diff's column (`Requests::since_bar`): on a request's versions compared,
/// while they are one of its comparisons.
pub fn since_piece(requests: Entity<Requests>, services: Rc<Services>) -> artoo_shell_api::Piece {
    artoo_shell_api::Piece {
        id: "requests.since",
        slot: Slot::DiffTop,
        draw: Rc::new(move |route, _, cx| {
            RequestVersionsRoute::from_route(route)?;
            let diff = services.get::<Rc<dyn Review>>()?.shown(cx)?;
            let Header::Versions { from, to, .. } = &diff.header else {
                return None;
            };
            let acts = services.get::<Acts>()?;
            requests.read(cx).since_bar((from.number, to.number), &acts)
        }),
    }
}

impl Requests {
    /// The latest version's head of the open request.
    fn latest_head(&self) -> Option<String> {
        self.current
            .and_then(|iid| self.find(iid))
            .map(|m| m.head.clone())
            .filter(|h| !h.is_empty())
    }

    /// The commit discussion `t` was written on: the host's word, else (an older GitLab answer
    /// without `head_sha`) the version pushed last before its first note.
    pub fn written_commit(&self, t: &Thread) -> Option<String> {
        if let Some(c) = &t.commit {
            return Some(c.clone());
        }
        let iid = self.current?;
        let when = t.notes.first()?.created;
        match &self.versions {
            Some((for_iid, list)) if *for_iid == iid && list.len() > 1 => list
                .iter()
                .rev()
                .find(|v| v.time <= when)
                .map(|v| v.tip.to_string()),
            _ => None,
        }
    }

    /// Written on an earlier version than the one shown: its line number is that version's.
    pub fn thread_is_old(&self, t: &Thread) -> bool {
        if t.outdated {
            return true;
        }
        match (self.written_commit(t), self.latest_head()) {
            (Some(c), Some(head)) => !head.starts_with(c.as_str()) && !c.starts_with(&head),
            _ => false,
        }
    }

    /// Follows the line of every discussion written on an earlier version to the latest, in the
    /// background, in the repository at `root`; the threads and the versions must both be in.
    /// `Event::Histories` tells the tab.
    pub fn compute_histories(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let Some(iid) = self.current else {
            return;
        };
        let versions: Vec<(usize, git2::Oid)> = match &self.versions {
            Some((for_iid, list)) if *for_iid == iid => {
                list.iter().map(|v| (v.number, v.tip)).collect()
            }
            _ => return,
        };
        let old: Vec<(String, String, String, u32)> = self
            .threads
            .iter()
            .filter(|t| self.thread_is_old(t))
            .filter_map(|t| {
                Some((
                    t.id.clone(),
                    self.written_commit(t)?,
                    t.path.clone()?,
                    t.new_line?,
                ))
            })
            .collect();
        if old.is_empty() {
            self.histories.clear();
            return;
        }
        self.history_task = Some(cx.spawn(async move |this, cx| {
            let histories = cx
                .background_executor()
                .spawn(async move {
                    let Ok(repo) = Repo::open(&root) else {
                        return HashMap::new();
                    };
                    old.into_iter()
                        .filter_map(|(id, commit, path, line)| {
                            let at = versions
                                .iter()
                                .position(|(_, tip)| tip.to_string().starts_with(&commit))?;
                            Some((id, history(&repo, &versions, at, &path, line)))
                        })
                        .collect::<HashMap<_, _>>()
                })
                .await;
            this.update(cx, |rq, cx| {
                if rq.current == Some(iid) {
                    rq.histories = histories;
                    cx.emit(Event::Histories);
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The outdated discussions by the version they were written on, newest version first.
    pub fn outdated_by_version(&self) -> Vec<(Option<usize>, Vec<usize>)> {
        let mut groups: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
        for (ix, t) in self.threads.iter().enumerate() {
            if t.path.is_none() || !self.thread_is_old(t) {
                continue;
            }
            let version = self.histories.get(&t.id).map(|h| h.written);
            match groups.iter_mut().find(|(v, _)| *v == version) {
                Some((_, list)) => list.push(ix),
                None => groups.push((version, vec![ix])),
            }
        }
        groups.sort_by_key(|(v, _)| std::cmp::Reverse(*v));
        groups
    }

    /// Whether thread `t` began with a comment of yours: only then is it "your comment".
    pub fn yours(&self, t: &Thread) -> bool {
        let me = self.login.as_deref().unwrap_or_default();
        !me.is_empty() && t.notes.first().is_some_and(|n| n.login == me)
    }

    /// What became of thread `ix`'s line, as the Outdated list says it under the comment: `No changes since v1`,
    /// `Changed since v1`, `Line removed since v1`; None while it is not known.
    pub fn since_comment(&self, ix: usize) -> Option<String> {
        let t = self.threads.get(ix)?;
        let h = self.histories.get(&t.id)?;
        let since = h.written;
        Some(match (h.changed(), h.line_now) {
            (_, None) => format!("Line removed since v{since}"),
            (true, _) => format!("Changed since v{since}"),
            (false, _) => format!("No changes since v{since}"),
        })
    }

    /// The button that compares thread `ix`'s version with the latest (`latest`, the timeline's
    /// last): "Compare v1 → v3"; none while they are one version.
    pub fn compare_label(&self, ix: usize, latest: Option<usize>) -> Option<String> {
        let t = self.threads.get(ix)?;
        let written = self.histories.get(&t.id)?.written;
        let latest = latest.filter(|l| *l > written)?;
        Some(format!("Compare v{written} → v{latest}"))
    }

    /// What became of an outdated discussion's line, as notes of its thread after its own (the owner, 2026-11-27: a
    /// comment saying what the code became, not a box over it), their faces on the thread's line: one for each version
    /// that changed the line — who, in which version and commit, and the line before and after, the changed words
    /// marked — or one under a plain heading saying it is unchanged, the line as it is now. Each note's code folds
    /// with its chevron; the last has Compare with the new version (`latest`, the timeline's last), which asks the
    /// tab, and ends the thread's line (the owner, 2026-11-27).
    pub fn render_history(&self, ix: usize, latest: Option<usize>, acts: Acts) -> Vec<AnyElement> {
        let Some(t) = self.threads.get(ix) else {
            return Vec::new();
        };
        let Some(h) = self.histories.get(&t.id).cloned() else {
            return Vec::new();
        };
        let written = h.written;
        // Compare, then the chevron that folds the note's code: at the end of its heading.
        let compare = |acts: &Acts| {
            self.compare_label(ix, latest).map(|label| {
                let acts = acts.clone();
                div()
                    .id(("compare-since", ix))
                    .px(px(8.))
                    .h(px(22.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme::island_border())
                    .text_size(px(12.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(
                        artoo_ui_kit::icons::icon("git-compare")
                            .size(px(12.))
                            .text_color(theme::muted()),
                    )
                    .child(label)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        acts(Act::CompareSince(ix), cx)
                    })
                    .into_any_element()
            })
        };
        let fold = |key: &str, open: bool, acts: &Acts| {
            let (acts, key) = (acts.clone(), key.to_owned());
            div()
                .id(SharedString::from(format!("history-fold-{key}")))
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(discussion::fold_chevron(open))
                .tip(if open {
                    "Fold the code"
                } else {
                    "Show the code"
                })
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    acts(Act::FoldHistory(key.clone()), cx)
                })
                .into_any_element()
        };
        let end = |key: &str, open: bool, last: bool| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .children(if last { compare(&acts) } else { None })
                .child(fold(key, open, &acts))
                .into_any_element()
        };
        // Each note on the thread's line: as far in as the notes over it, its face under theirs.
        let on_line = |note: gpui::Div| div().pl(px(GUTTER)).child(note).into_any_element();
        if !h.changed() {
            let key = format!("{}:same", t.id);
            let open = !self.history_folded.contains(&key);
            let span = match latest {
                Some(latest) if latest > written => format!("v{written} → v{latest}"),
                _ => format!("written on v{written}"),
            };
            let now = h.line_now.or(t.new_line);
            let line = h.then.trim_start().to_owned();
            let body = open.then(|| {
                code_block(
                    &key,
                    vec![CodeRow {
                        number: now,
                        sign: None,
                        plain: line.clone(),
                        shown: line.into_any_element(),
                    }],
                )
            });
            return vec![on_line(discussion::change_note(
                format!("outdated-{ix}-same"),
                "No changes to this line".to_owned(),
                span,
                body,
                true,
                Some(end(&key, open, true)),
            ))];
        }
        let last = h.steps.len() - 1;
        let mut before = (h.then.clone(), t.new_line);
        // Each version's line against the one before it: the old line and the new, the changed words marked (#46 B).
        h.steps
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let tag = format!("v{}", s.version);
                let key = format!("{}:{tag}", t.id);
                let open = !self.history_folded.contains(&key);
                let (old, old_line) = before.clone();
                let rows = match &s.now {
                    Some(now) => {
                        before = (now.clone(), s.line);
                        // The indent both share left out: the code starts by its sign.
                        let cut = indent(&old).min(indent(now));
                        let (old, now) = (&old[cut..], &now[cut..]);
                        let pieces = worddiff::pieces(old, now);
                        vec![
                            CodeRow {
                                number: old_line,
                                sign: Some(false),
                                plain: old.to_owned(),
                                shown: side_marked(old, pieces.as_deref(), true),
                            },
                            CodeRow {
                                number: s.line,
                                sign: Some(true),
                                plain: now.to_owned(),
                                shown: side_marked(now, pieces.as_deref(), false),
                            },
                        ]
                    }
                    None => vec![CodeRow {
                        number: old_line,
                        sign: Some(false),
                        plain: old.trim_start().to_owned(),
                        shown: old.trim_start().to_owned().into_any_element(),
                    }],
                };
                let did = match s.now {
                    Some(_) => format!("changed it in v{}", s.version),
                    None => format!("removed it in v{}", s.version),
                };
                let what = if s.sha.is_empty() {
                    did
                } else {
                    format!("{did} · {} {}", s.sha, s.summary)
                };
                on_line(discussion::change_note(
                    format!("outdated-{ix}-{tag}"),
                    if s.who.is_empty() {
                        "The line".to_owned()
                    } else {
                        s.who.clone()
                    },
                    what,
                    open.then(|| code_block(&key, rows)),
                    k == last,
                    Some(end(&key, open, k == last)),
                ))
            })
            .collect()
    }
}

/// A line of a history note's code: its number, `−` (`Some(false)`) or `+` (`Some(true)`) or none, its text measured
/// and as shown.
struct CodeRow {
    number: Option<u32>,
    sign: Option<bool>,
    plain: String,
    shown: AnyElement,
}

/// A history note's code as a diff draws it: a framed block, each line with its number and sign on its side's tint.
fn code_block(key: &str, rows: Vec<CodeRow>) -> AnyElement {
    div()
        .max_w_full()
        .py(px(2.))
        .flex()
        .flex_col()
        .rounded(px(6.))
        .border_1()
        .border_color(theme::island_border())
        .bg(theme::editor())
        .overflow_hidden()
        .font_family(theme::code_font())
        .text_size(px(12.))
        .line_height(px(20.))
        .children(rows.into_iter().enumerate().map(|(n, row)| {
            let (sign, tint, ink) = match row.sign {
                Some(false) => ("−", Some(theme::removed_line()), theme::removed()),
                Some(true) => ("+", Some(theme::added_line()), theme::added()),
                None => ("", None, theme::faint()),
            };
            div()
                .flex()
                .items_center()
                .min_w_0()
                .when_some(tint, |s, t| s.bg(t))
                .child(
                    div()
                        .w(px(36.))
                        .flex_none()
                        .pr(px(6.))
                        .flex()
                        .justify_end()
                        .text_color(theme::faint())
                        .child(row.number.map(|l| l.to_string()).unwrap_or_default()),
                )
                .child(div().w(px(16.)).flex_none().text_color(ink).child(sign))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .pr(px(8.))
                        .text_color(theme::text())
                        .child(hover::cut_with(
                            SharedString::from(format!("history-{key}-{n}")),
                            row.plain,
                            row.shown,
                        )),
                )
        }))
        .into_any_element()
}

/// One side of a changed line with what changed on it marked: the old side's removed words, the new side's added ones;
/// a line rewritten past recognition (no `pieces`) reads whole.
fn side_marked(text: &str, pieces: Option<&[Piece]>, old: bool) -> AnyElement {
    let Some(pieces) = pieces else {
        return text.to_owned().into_any_element();
    };
    let tint = if old {
        theme::removed_word()
    } else {
        theme::added_word()
    };
    let runs: Vec<_> = changed(pieces, old)
        .into_iter()
        .filter(|r| r.end <= text.len())
        .map(|r| {
            (
                r,
                HighlightStyle {
                    background_color: Some(tint.into()),
                    ..Default::default()
                },
            )
        })
        .collect();
    StyledText::new(text.to_owned())
        .with_highlights(runs)
        .into_any_element()
}

/// How far `line` is indented, in bytes of its leading spaces and tabs.
fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// Where one side of a changed line changed: the old side's ranges (`old`), else the new side's.
fn changed(pieces: &[Piece], old: bool) -> Vec<std::ops::Range<usize>> {
    pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Change(o, n) => Some(if old { o.clone() } else { n.clone() }),
            Piece::Same(..) => None,
        })
        .filter(|r| !r.is_empty())
        .collect()
}

/// Follows `line` of `path` from version `at` to the last of `versions`.
fn history(
    repo: &Repo,
    versions: &[(usize, git2::Oid)],
    at: usize,
    path: &str,
    line: u32,
) -> LineHistory {
    let (written, tip) = versions[at];
    let then = repo
        .file_at(tip, path)
        .and_then(|t| {
            t.lines()
                .nth(line as usize - 1)
                .map(|l| l.trim_end().to_owned())
        })
        .unwrap_or_default();
    let mut steps = Vec::new();
    let mut now = Some(line);
    for pair in versions[at..].windows(2) {
        let ((_, old), (number, new)) = (pair[0], pair[1]);
        let Some(cur) = now else { break };
        let who = || {
            repo.last_change(old, new, path)
                .map(|c| (c.author, artoo_ui_kit::format::short(c.id), c.summary))
                .unwrap_or_default()
        };
        match repo.follow_line(old, new, path, cur) {
            Fate::Same(n) => now = Some(n),
            Fate::Changed(n, text) => {
                let (who, sha, summary) = who();
                steps.push(Step {
                    version: number,
                    who,
                    summary,
                    sha,
                    now: Some(text),
                    line: Some(n),
                });
                now = Some(n);
            }
            Fate::Gone => {
                let (who, sha, summary) = who();
                steps.push(Step {
                    version: number,
                    who,
                    summary,
                    sha,
                    now: None,
                    line: None,
                });
                now = None;
            }
        }
    }
    LineHistory {
        written,
        then,
        steps,
        line_now: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn a_changed_line_marks_what_went_on_the_old_side_and_what_came_on_the_new() {
        let (old, new) = (
            "val result = api.login(form)",
            "val result = runCatching { api.login(form) }",
        );
        let pieces = worddiff::pieces(old, new).expect("alike lines");
        let came: Vec<&str> = changed(&pieces, false)
            .into_iter()
            .map(|r| &new[r])
            .collect();
        assert!(came.iter().any(|c| c.contains("runCatching")));
        // Nothing of the old line went: only words came.
        assert!(changed(&pieces, true).is_empty());
    }
}
