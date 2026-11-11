//! A branch's or a request's Commits (#73, #74): its commits, newest first, with the version each came with when it
//! has versions; a branch's as a view with its line over them (`BranchCommits`). One is picked — the one esc came back from, else the newest —, ↑ ↓ or a click pick another, shift
//! and a click a run of them; ↵ or a second click opens one as a level (`Act::Commit`), the run as one diff.

use crate::history::{Act, Acts, History};
use artoo_core::route::Route;
use artoo_shell_api::Frame;
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover, RowTip};
use artoo_ui_kit::{gap, icons, island, px, row, theme};
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, MouseButton, MouseDownEvent, Render, Window, div,
    prelude::*,
};
use std::rc::{Rc, Weak};

/// A branch's Commits off the main line (#74), a view the tab puts under the branch's bar: its line over its
/// commits.
pub struct BranchCommits {
    history: Entity<History>,
    frame: Weak<dyn Frame>,
    /// The route on screen.
    pub route: Option<Route>,
}

/// The tab's view of it, made with its parts, among its services.
pub fn view(
    history: Entity<History>,
    frame: &Rc<dyn Frame>,
    cx: &mut App,
) -> Entity<BranchCommits> {
    let frame = Rc::downgrade(frame);
    cx.new(|_| BranchCommits {
        history,
        frame,
        route: None,
    })
}

impl Render for BranchCommits {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (Some(frame), Some(route)) = (self.frame.upgrade(), self.route.as_ref()) else {
            return div().flex_1();
        };
        let services = frame.services();
        let line = crate::facts::line_of(&self.history, &services, route, cx)
            .unwrap_or_else(|| div().into_any_element());
        let acts = services.get::<Acts>().unwrap_or_else(|| Rc::new(|_, _| {}));
        let commits = self
            .history
            .update(cx, |h, cx| h.commits_page(false, &acts, cx));
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(gap()))
            .child(line)
            .child(commits)
    }
}

impl History {
    /// The commit picked: the one esc came back from, else the newest.
    pub fn picked(&self) -> Option<usize> {
        let at = self
            .pick
            .and_then(|id| self.commits.iter().position(|c| c.id == id));
        at.or((!self.commits.is_empty()).then_some(0))
    }

    /// The run of commits picked with shift: the newest's index and the oldest's.
    pub fn picked_run(&self) -> Option<(usize, usize)> {
        let at = |id: git2::Oid| self.commits.iter().position(|c| c.id == id);
        let a = at(self.pick?)?;
        let b = at(self.pick_to?)?;
        (a != b).then_some((a.min(b), a.max(b)))
    }

    /// Commit `id` picked, a run to `to` with it: a commit's page or a run's on screen, esc comes back to it.
    pub fn pick_commits(&mut self, id: git2::Oid, to: Option<git2::Oid>) {
        self.pick = Some(id);
        self.pick_to = to;
    }

    /// ↑ ↓: another commit picked, nothing opened.
    pub fn step_pick(&mut self, down: bool) {
        let n = self.commits.len();
        let to = match (self.picked(), down) {
            (Some(at), true) => (at + 1).min(n.saturating_sub(1)),
            (Some(at), false) => at.saturating_sub(1),
            (None, _) => return,
        };
        self.pick = self.commits.get(to).map(|c| c.id);
        self.pick_to = None;
    }

    /// Commit `ix` clicked, with shift or not: picked, or the run from the one picked to it. Whether it opens:
    /// clicked again.
    fn click_pick(&mut self, ix: usize, shift: bool, clicks: usize) -> bool {
        let id = self.commits.get(ix).map(|c| c.id);
        if shift {
            if self.pick.is_none() {
                self.pick = self
                    .picked()
                    .and_then(|at| self.commits.get(at).map(|c| c.id));
            }
            self.pick_to = id;
            return false;
        }
        if self.picked() == Some(ix) && clicks > 1 {
            return true;
        }
        self.pick = id;
        self.pick_to = None;
        false
    }

    /// The commits as a page of their own, with the version each came with when `versions`.
    pub fn commits_page(&self, versions: bool, acts: &Acts, cx: &mut Context<Self>) -> AnyElement {
        let came = if versions {
            let tips: Vec<(usize, git2::Oid)> =
                self.versions.iter().map(|v| (v.number, v.tip)).collect();
            let ids: Vec<git2::Oid> = self.commits.iter().map(|c| c.id).collect();
            commit_versions(&ids, &tips)
        } else {
            vec![None; self.commits.len()]
        };
        let picked = self.picked();
        // Shift picks a run of them, to review as one diff.
        let run = self.picked_run();
        // A commit's anatomy (#79): its sha and title always; when, and who when there are
        // several, while the list has 260 px; the icon and its version under 360 px leave first.
        // The tooltip holds them all.
        let room = self.widths.room("commits");
        let several = self
            .commits
            .first()
            .is_some_and(|first| self.commits.iter().any(|c| c.author != first.author));
        let this = cx.entity().downgrade();
        let rows: Vec<_> = self
            .commits
            .iter()
            .enumerate()
            .map(|(ix, c)| {
                let selected = match run {
                    Some((newest, oldest)) => (newest..=oldest).contains(&ix),
                    None => picked == Some(ix),
                };
                let mut tip = vec![
                    c.summary.clone(),
                    format!(
                        "{} · {} · {}",
                        format::short(c.id),
                        c.author,
                        hover::full_date(c.time)
                    ),
                ];
                if let Some(v) = came[ix] {
                    tip.push(format!("came with v{v}"));
                }
                let (menu, open, this) = (acts.clone(), acts.clone(), this.clone());
                row(("request-commit", ix), selected)
                    .h(px(36.))
                    .gap(px(10.))
                    .px(px(10.))
                    .when(room.third(), |s| {
                        s.child(
                            icons::icon("commit")
                                .size(px(14.))
                                .text_color(theme::faint()),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme::code_font())
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child(format::short(c.id)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .cut_in_tip()
                            .child(c.summary.clone()),
                    )
                    .children((several && room.second()).then(|| {
                        div()
                            .flex_none()
                            .max_w(px(140.))
                            .cut_in_tip()
                            .text_size(px(11.))
                            .text_color(theme::muted())
                            .child(c.author.clone())
                    }))
                    .children(came[ix].filter(|_| room.third()).map(|v| {
                        div()
                            .flex_none()
                            .px(px(6.))
                            .rounded_full()
                            .border_1()
                            .border_color(theme::accent())
                            .text_size(px(11.))
                            .text_color(theme::accent())
                            .child(format!("v{v}"))
                    }))
                    .when(room.second(), |s| {
                        s.child(
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format::ago(c.time)),
                        )
                    })
                    .row_tip(selected, tip.join("\n"))
                    // Its SHA, its message, its link (#162).
                    .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, _, cx| {
                        menu(Act::CommitMenu { ix, at: e.position }, cx)
                    })
                    .on_click(move |event: &ClickEvent, _, cx| {
                        let (shift, clicks) = (event.modifiers().shift, event.click_count());
                        let opens = this
                            .update(cx, |h, cx| {
                                let opens = h.click_pick(ix, shift, clicks);
                                cx.notify();
                                opens
                            })
                            .unwrap_or(false);
                        if opens {
                            open(Act::Commit(ix), cx)
                        }
                    })
            })
            .collect();
        island()
            .flex_1()
            .min_h_0()
            .bg(theme::editor())
            .child(
                div()
                    .id("request-commits")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(8.))
                    .py(px(8.))
                    .child(self.widths.measure("commits"))
                    .child(
                        div()
                            .id("commits-count")
                            .px(px(10.))
                            .pb(px(6.))
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(match run {
                                Some((newest, oldest)) => format!(
                                    "{} of {} picked · ↵ reviews them as one",
                                    oldest - newest + 1,
                                    plural(self.commits.len(), "commit")
                                ),
                                None => plural(self.commits.len(), "commit"),
                            })
                            .tip("Newest first · ↵ opens one, esc comes back here · shift and a click picks a run of them to review as one"),
                    )
                    .children(rows),
            )
            .into_any_element()
    }
}

/// Each commit's version, the commits newest first: the version whose tip it is, else the one of
/// the commit above it — it came with that push. None above the first tip found (a push rewrote
/// what the versions knew).
fn commit_versions(commits: &[git2::Oid], tips: &[(usize, git2::Oid)]) -> Vec<Option<usize>> {
    let mut at = None;
    commits
        .iter()
        .map(|id| {
            // The first push it was the tip of: when it came.
            if let Some(&(number, _)) = tips.iter().find(|(_, tip)| tip == id) {
                at = Some(number);
            }
            at
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::commit_versions;
    use git2::Oid;

    fn id(n: u8) -> Oid {
        Oid::from_bytes(&[n; 20]).unwrap()
    }

    #[test]
    fn a_commit_is_of_the_push_it_came_with() {
        // Newest first: 3 is v3's tip, 2 v2's, 1 came with v2 too (v1's tip was 0).
        let commits = [id(3), id(2), id(1), id(0)];
        let tips = [(1, id(0)), (2, id(2)), (3, id(3))];
        assert_eq!(
            commit_versions(&commits, &tips),
            [Some(3), Some(2), Some(2), Some(1)]
        );
        // A push rewrote them all: none of the tips is among them but the last.
        let rewritten = [id(9), id(8), id(7)];
        assert_eq!(
            commit_versions(&rewritten, &[(1, id(0)), (2, id(9))]),
            [Some(2), Some(2), Some(2)]
        );
        assert_eq!(commit_versions(&[id(1)], &[]), [None]);
    }
}
