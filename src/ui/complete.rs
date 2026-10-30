//! Completion while writing a comment (#51 mentions, #9 `:name` emoji): who and which emoji it
//! lists is the requests' (`complete`), the list and its keys the kit's.

use super::Workspace;
pub(super) use artoo_ui_kit::complete::{Choice, render_list};
use gpui::App;

impl Workspace {
    /// The list for the token before `caret` in `text`, unless it was closed with esc at `closed`
    /// (`Requests::completion`).
    pub(super) fn completion(
        &self,
        text: &str,
        caret: usize,
        closed: Option<usize>,
        cx: &App,
    ) -> Option<(usize, Vec<Choice>)> {
        self.requests.read(cx).completion(text, caret, closed)
    }

    /// The username of the person called `name`, as the request and the project know them.
    pub(super) fn login_of(&self, name: &str, cx: &App) -> Option<String> {
        self.requests
            .read(cx)
            .people()
            .into_iter()
            .find(|(l, n)| n == name || l == name)
            .map(|(l, _)| l)
    }
}
