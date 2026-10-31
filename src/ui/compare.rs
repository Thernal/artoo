//! A comparison's commits (#21, B): Commits · N over Changes in the side panel of a comparison's
//! own page, newest first. A click narrows the diff to that commit's own — against its first
//! parent, a merge's too —, a ⇧-click to a run of them (the oldest's parent against the newest),
//! as a branch's Commits picks; "a41f9c0 only" over the diff says so, and esc or its × bring the
//! whole comparison back. The page stays the comparison's: picking is a filter, not a level.
//! Its rows and the pill are the branches' (`artoo_branches::side`, `compare`); the tab reads and picks.

use super::nav::Page;
use super::{Selection, Workspace};
use artoo_branches::compare::{CompareCommits, Sides};
use artoo_branches::side::{compare_commits, compare_row};
use gpui::{AnyElement, App, Context, prelude::*};

impl Workspace {
    /// The comparison's commits, while its page is on screen and they are read.
    pub(super) fn shown_compare<'a>(&self, cx: &'a App) -> Option<&'a CompareCommits> {
        self.history
            .read(cx)
            .compare
            .as_ref()
            .filter(|c| c.list.as_ref().is_some_and(|l| !l.is_empty()))
            .filter(|c| is_page(&c.of, self.nav_page()))
    }

    /// The comparison's page while a run of its commits narrows its diff.
    pub(super) fn narrowed_compare(&self, cx: &App) -> Option<Page> {
        self.shown_compare(cx)
            .filter(|c| c.picked.is_some() && self.history.read(cx).selection == Selection::None)
            .map(|c| Page::Compare(c.of.0.clone(), c.of.1.clone(), c.of.2))
    }

    /// A comparison of `page` shown whole: its commits are read, unless they are these already
    /// (esc back from a pick); none picked. Any other diff (`None`) has none.
    pub(super) fn read_compare_commits(&mut self, page: Option<Page>, cx: &mut Context<Self>) {
        let (Some(Page::Compare(base, head, since)), Some(root)) = (page, self.root.clone()) else {
            self.history.update(cx, |h, _| h.compare = None);
            return;
        };
        self.history
            .update(cx, |h, cx| h.read_compare(root, (base, head, since), cx));
    }

    /// A click on commit `ix`: its diff alone; with ⇧ the run from the one clicked last to it.
    pub(super) fn pick_compare_commit(&mut self, ix: usize, span: bool, cx: &mut Context<Self>) {
        let picked = self
            .history
            .update(cx, |h, _| h.compare.as_mut().map(|c| c.pick(ix, span)));
        if picked.is_none() {
            return;
        }
        let keep = self.review.read(cx).current_path();
        self.load_compare_pick(keep, cx);
    }

    /// ↑ ↓ in the comparison's commits: the one above or below the one picked.
    pub(super) fn step_compare_pick(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(to) = self.shown_compare(cx).and_then(|c| c.step(down)) else {
            return;
        };
        self.pick_compare_commit(to, false, cx);
        self.compare_scroll
            .scroll_to_item(to, gpui::ScrollStrategy::Nearest);
    }

    /// Shows the run picked under the comparison's own page (`History::show_compare_pick`); `keep` stays on that
    /// file.
    pub(super) fn load_compare_pick(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        if self.root.is_none() {
            return;
        }
        let review = self.review_handle();
        if self
            .history
            .update(cx, |h, cx| h.show_compare_pick(keep, &*review, cx))
        {
            self.body_expanded = false;
            cx.notify();
        }
    }

    /// esc or the pill's ×: the whole comparison again. Whether a run was picked.
    pub(super) fn unpick_compare(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(Page::Compare(base, head, since)) = self.narrowed_compare(cx) else {
            return false;
        };
        self.run_compare(base, head, since, None, cx);
        true
    }

    /// The title of a comparison narrowed to one of its commits: the commit's subject.
    pub(super) fn compare_title(&self, cx: &App) -> Option<String> {
        self.narrowed_compare(cx)?;
        let c = self.history.read(cx).compare.as_ref()?;
        match c.picked? {
            (newest, oldest) if newest == oldest => {
                Some(c.list.as_ref()?.get(newest)?.summary.clone())
            }
            _ => None,
        }
    }

    /// The Commits section's line when it is folded: how many are picked, else the newest.
    pub(super) fn compare_summary(&self, cx: &App) -> Option<String> {
        let c = self.shown_compare(cx)?;
        let list = c.list.as_ref()?;
        Some(match c.picked {
            Some((newest, oldest)) => format!("{} of {}", oldest - newest + 1, list.len()),
            None if list.len() > 1 => format!("{} +{}", list[0].summary, list.len() - 1),
            None => list[0].summary.clone(),
        })
    }

    /// The comparison's commits: a row each — its card on a long hover —, as many as there are room for before
    /// the list scrolls.
    pub(super) fn render_compare_commits(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let n = self.shown_compare(cx)?.list.as_ref()?.len();
        let list = gpui::uniform_list(
            "compare-commits",
            n,
            cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                let acts = this.history_acts(cx);
                let compare = this.history.read(cx).compare.as_ref();
                range
                    .map(|ix| {
                        let card = compare
                            .and_then(|c| c.list.as_ref()?.get(ix))
                            .map(|info| this.card_of(info.clone(), cx));
                        compare_row(compare, ix, card, &acts)
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.compare_scroll);
        Some(compare_commits(list, n).into_any_element())
    }
}

/// Whether `page` is the comparison `of`.
fn is_page(of: &Sides, page: Option<&Page>) -> bool {
    matches!(page, Some(Page::Compare(base, head, since)) if (base, head, since) == (&of.0, &of.1, &of.2))
}
