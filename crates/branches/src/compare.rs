//! A comparison's own commits (#21, B), read with it — the history's own task —: the run of them its diff is
//! narrowed to, what a click or an arrow picks, and the pill that says so over the diff.

use crate::history::{Act, Acts, History, NamedTip};
use artoo_core::git::{CommitInfo, Repo};
use artoo_ui_kit::hover::Hover;
use artoo_ui_kit::{format, icons, keys, px, theme};
use gpui::{ClickEvent, Context, Div, div, prelude::*};

/// Two refs compared: the base, the head, and whether since their merge base.
pub type Sides = (NamedTip, NamedTip, bool);

/// A comparison's own commits, read with it.
pub struct CompareCommits {
    /// The comparison they are of.
    pub of: Sides,
    /// Newest first; None while they are read.
    pub list: Option<Vec<CommitInfo>>,
    /// The run whose diff is shown — the newest's index and the oldest's —; None the whole.
    pub picked: Option<(usize, usize)>,
    /// Where a ⇧-click's run starts: the commit clicked last.
    anchor: Option<usize>,
}

impl CompareCommits {
    /// The commits of the comparison `of`, before they are read.
    pub fn of(of: Sides) -> Self {
        CompareCommits {
            of,
            list: None,
            picked: None,
            anchor: None,
        }
    }

    /// The commits picked, newest first.
    pub fn picked_ids(&self) -> Option<(git2::Oid, git2::Oid)> {
        let (newest, oldest) = self.picked?;
        let list = self.list.as_ref()?;
        Some((list.get(newest)?.id, list.get(oldest)?.id))
    }

    /// A click on commit `ix`: it alone; with ⇧ the run from the one clicked last to it.
    pub fn pick(&mut self, ix: usize, span: bool) {
        self.picked = Some(run_to(self.anchor, ix, span));
        if !span || self.anchor.is_none() {
            self.anchor = Some(ix);
        }
    }

    /// ↑ ↓: the one commit to pick past the run, if any.
    pub fn step(&self, down: bool) -> Option<usize> {
        step_to(self.picked, down, self.list.as_ref().map_or(0, Vec::len))
    }
}

impl History {
    /// The commits of the comparison `of`, read unless they are these already (esc back from a pick); none
    /// picked.
    pub fn read_compare(&mut self, of: Sides, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            self.compare = None;
            return;
        };
        if let Some(c) = self.compare.as_mut().filter(|c| c.of == of) {
            c.picked = None;
            return;
        }
        let (from, to, since) = (of.0.1, of.1.1, of.2);
        self.compare = Some(CompareCommits::of(of.clone()));
        cx.spawn(async move |this, cx| {
            let list = cx
                .background_executor()
                .spawn(async move { Repo::open(&root).and_then(|r| r.range_log(from, to, since)) })
                .await
                .unwrap_or_default();
            this.update(cx, |h, cx| {
                if let Some(c) = h.compare.as_mut().filter(|c| c.of == of) {
                    c.list = Some(list);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

/// The run a click on `ix` picks — the newest's index and the oldest's —: with ⇧ from `anchor`,
/// the one clicked last, to it; else it alone.
fn run_to(anchor: Option<usize>, ix: usize, span: bool) -> (usize, usize) {
    match (span, anchor) {
        (true, Some(at)) => (at.min(ix), at.max(ix)),
        _ => (ix, ix),
    }
}

/// ↑ ↓ from the run `picked` in a list of `len`, newest first: the one commit to pick — the first
/// when none is, the one past the run's end the key points to, which stops at the list's ends.
fn step_to(picked: Option<(usize, usize)>, down: bool, len: usize) -> Option<usize> {
    let last = len.checked_sub(1)?;
    match (picked, down) {
        (None, true) => Some(0),
        (None, false) => None,
        (Some((newest, _)), false) => Some(newest.saturating_sub(1)),
        (Some((_, oldest)), true) => Some((oldest + 1).min(last)),
    }
}

/// Over the diff narrowed to the run `c` picked: "a41f9c0 only" (a run its ends), its × the whole comparison.
pub fn only_pill(c: &CompareCommits, acts: &Acts) -> Option<Div> {
    let (newest, oldest) = c.picked_ids()?;
    let what = if newest == oldest {
        format::short(newest)
    } else {
        format!("{} … {}", format::short(oldest), format::short(newest))
    };
    let acts = acts.clone();
    Some(
        div()
            .flex_none()
            .h(px(24.))
            .pl(px(8.))
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded_full()
            .bg(theme::selected())
            .text_size(px(12.))
            .child(
                icons::icon("commit")
                    .size(px(13.))
                    .text_color(theme::muted()),
            )
            .child(div().font_family(theme::code_font()).child(what))
            .child("only")
            .child(
                div()
                    .id("compare-whole")
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(icons::icon("x").size(px(12.)).text_color(theme::muted()))
                    .tip(keys::text("The whole comparison  esc").into_owned())
                    .on_click(move |_: &ClickEvent, _, cx| acts(Act::WholeCompare, cx)),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_picks_one_and_a_shift_click_the_run_from_the_last_one() {
        assert_eq!(run_to(None, 2, false), (2, 2));
        assert_eq!(run_to(Some(1), 3, false), (3, 3));
        // Newest first: the run's newest is the smaller index, whichever was clicked first.
        assert_eq!(run_to(Some(1), 3, true), (1, 3));
        assert_eq!(run_to(Some(3), 1, true), (1, 3));
        assert_eq!(run_to(None, 2, true), (2, 2));
    }

    #[test]
    fn the_arrows_pick_the_next_commit_past_the_run() {
        assert_eq!(step_to(None, true, 5), Some(0));
        assert_eq!(step_to(None, false, 5), None);
        assert_eq!(step_to(Some((1, 3)), true, 5), Some(4));
        assert_eq!(step_to(Some((1, 3)), false, 5), Some(0));
        assert_eq!(step_to(Some((4, 4)), true, 5), Some(4));
        assert_eq!(step_to(Some((0, 0)), false, 5), Some(0));
        assert_eq!(step_to(None, true, 0), None);
    }
}
