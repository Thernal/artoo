//! A branch's page (#74): a branch that forks from the main line has the tabs Commits · Changes,
//! Changes being the branch against the main line since they parted, with one line of facts over
//! it — how far ahead and behind, whether it is pushed, its request — and Create merge request as
//! its primary action. A commit of it is a level under Commits; a run of its commits picked with
//! shift is one diff there. Its pages carry review marks, so your own work is reviewed before it
//! goes out. The main line itself has no base: its page is the commit browser it always was.

use super::nav::Page;
use super::{Header, Workspace};
use artoo_branches::facts::branch_line;
use gpui::{AnyElement, App, Context, prelude::*};

impl Workspace {
    /// The branch loaded has the tabs Commits · Changes: it forks from the main line.
    pub(super) fn branch_tabbed(&self, refname: &str, cx: &App) -> bool {
        self.history
            .read(cx)
            .page
            .as_ref()
            .is_some_and(|f| f.refname == refname && f.base.is_some())
    }

    /// The diff loaded is the branch page's own: the branch against the main line.
    pub(super) fn branch_compare(&self, cx: &App) -> bool {
        let (Some(f), Some(ix)) = (
            self.history.read(cx).page.as_ref(),
            self.history.read(cx).branch,
        ) else {
            return false;
        };
        matches!(
            (self.review.read(cx).diff.as_ref().map(|d| &d.header), &f.base, self.history.read(cx).branches.get(ix)),
            (
                Some(Header::Compare { base, head, request: None, .. }),
                Some(main),
                Some(b),
            ) if base.1 == main.1 && head.1 == b.tip
        )
    }

    /// Shows the branch loaded against the main line since they parted: its Changes.
    pub(super) fn run_branch_compare(&mut self, cx: &mut Context<Self>) {
        let (Some(ix), Some(base)) = (
            self.history.read(cx).branch,
            self.history
                .read(cx)
                .page
                .as_ref()
                .and_then(|f| f.base.clone()),
        ) else {
            return;
        };
        let Some(b) = self.history.read(cx).branches.get(ix) else {
            return;
        };
        let head = (b.name.clone(), b.tip);
        self.run_compare(base, head, true, None, cx);
    }

    /// One line over a branch's Changes and Commits, on its page: the branches' (`artoo_branches::facts`).
    pub(super) fn render_branch_line(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Some(Page::Branch(refname)) = self.nav_page() else {
            return None;
        };
        if !self.branch_tabbed(refname, cx) {
            return None;
        }
        let ix = self.history.read(cx).branch;
        let request = ix
            .and_then(|ix| self.request_of_branch(ix, cx))
            .map(|m| m.iid);
        let acts = self.history_acts(cx);
        let line = branch_line(self.history.read(cx).page.as_ref()?, ix, request, &acts)?;
        Some(line.into_any_element())
    }
}
