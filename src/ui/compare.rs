//! A comparison's commits (#21, B): Commits · N over Changes in the side panel of a comparison's
//! own page, newest first. A click narrows the diff to that commit's own — against its first
//! parent, a merge's too —, a ⇧-click to a run of them (the oldest's parent against the newest),
//! as a branch's Commits picks; "a41f9c0 only" over the diff says so, and esc or its × bring the
//! whole comparison back. The page stays the comparison's: picking is a filter, not a level.
//! Its rows, the pill and what is picked are the branches' (`artoo_branches::compare`); the tab asks them.

use super::Workspace;
use super::nav::Page;
use gpui::{App, Context};

impl Workspace {
    /// The comparison's page is on screen, its commits read.
    pub(super) fn shown_compare(&self, cx: &App) -> bool {
        self.branches()
            .is_some_and(|b| b.compare_listed(self.nav_page(), cx))
    }

    /// The comparison's page while a run of its commits narrows its diff.
    pub(super) fn narrowed_compare(&self, cx: &App) -> Option<Page> {
        let route = self.nav_page()?;
        self.branches()
            .is_some_and(|b| b.compare_narrowed(Some(route), cx))
            .then(|| Page::from_route(route))
            .flatten()
    }

    /// A comparison of `page` shown whole: its commits are read, unless they are these already
    /// (esc back from a pick); none picked. Any other diff (`None`) has none.
    pub(super) fn read_compare_commits(&mut self, page: Option<Page>, cx: &mut Context<Self>) {
        if let Some(branches) = self.branches() {
            branches.read_compare(page.as_ref().map(Page::route).as_ref(), cx);
        }
    }

    /// A click on commit `ix`: its diff alone; with ⇧ the run from the one clicked last to it.
    pub(super) fn pick_compare_commit(&mut self, ix: usize, span: bool, cx: &mut Context<Self>) {
        let keep = self.current_path(cx);
        if self.root.is_some()
            && self
                .branches()
                .is_some_and(|b| b.pick_compare(ix, span, keep, cx))
        {
            cx.notify();
        }
    }

    /// ↑ ↓ in the comparison's commits: the one above or below the one picked.
    pub(super) fn step_compare_pick(&mut self, down: bool, cx: &mut Context<Self>) {
        let route = self.nav_page().cloned();
        let keep = self.current_path(cx);
        if self.root.is_some()
            && self
                .branches()
                .is_some_and(|b| b.step_compare(route.as_ref(), down, keep, cx))
        {
            cx.notify();
        }
    }

    /// Shows the run picked under the comparison's own page (`Branches::show_compare_pick`); `keep` stays on that
    /// file.
    pub(super) fn load_compare_pick(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        if self.root.is_some()
            && self
                .branches()
                .is_some_and(|b| b.show_compare_pick(keep, cx))
        {
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
}
