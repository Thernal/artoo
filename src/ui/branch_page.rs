//! A branch's page (#74): a branch that forks from the main line has the tabs Commits · Changes,
//! Changes being the branch against the main line since they parted, with one line of facts over
//! it — how far ahead and behind, whether it is pushed, its request — and Create merge request as
//! its primary action. A commit of it is a level under Commits; a run of its commits picked with
//! shift is one diff there. Its pages carry review marks, so your own work is reviewed before it
//! goes out. The main line itself has no base: its page is the commit browser it always was.

use super::Workspace;
use gpui::Context;

impl Workspace {
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
}
