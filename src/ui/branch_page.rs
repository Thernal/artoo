//! A branch's page (#74): a branch that forks from the main line has the tabs Commits · Changes,
//! Changes being the branch against the main line since they parted, with one line of facts over
//! it — how far ahead and behind, whether it is pushed, its request — and Create merge request as
//! its primary action. A commit of it is a level under Commits; a run of its commits picked with
//! shift is one diff there. Its pages carry review marks, so your own work is reviewed before it
//! goes out. The main line itself has no base: its page is the commit browser it always was.

use super::Workspace;
use gpui::Context;

impl Workspace {
    /// Shows the branch loaded against the main line since they parted: its Changes
    /// (`History::show_branch_compare`).
    pub(super) fn run_branch_compare(&mut self, cx: &mut Context<Self>) {
        let review = self.review_handle();
        if self
            .history
            .update(cx, |h, cx| h.show_branch_compare(&*review, cx))
        {
            cx.notify();
        }
    }
}
