//! Completion while writing a comment (#51 mentions, #9 `:name` emoji) in a plain field: ↑ ↓ move,
//! ↵ or ⇥ puts it in, esc closes it. Who and which emoji it lists is the requests' (`complete`).

use super::Workspace;
pub(super) use artoo_ui_kit::complete::{Choice, list_key, render_list};
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

    /// The list for the token at the caret of a plain field showing `text` (the new request's
    /// description, a review's summary), if one is open.
    pub(super) fn field_completion(&self, text: &str, cx: &App) -> Option<(usize, Vec<Choice>)> {
        // Until the field is edited the shared edit holds another text: the caret is at the end.
        let caret = if self.field_edit.text() == text {
            self.field_edit.caret()
        } else {
            text.len()
        };
        self.completion(text, caret, self.field_closed, cx)
    }

    /// ↑ ↓ ↵ ⇥ esc in a plain field while its list is open; whether it took the key. `text` is the
    /// field's text, changed when a choice goes in.
    pub(super) fn field_completion_key(
        &mut self,
        text: &mut String,
        key: &gpui::Keystroke,
        cx: &App,
    ) -> bool {
        let Some((at, choices)) = self.field_completion(text, cx) else {
            return false;
        };
        list_key(
            key,
            text,
            &mut self.field_edit,
            &mut self.field_pick,
            &mut self.field_closed,
            at,
            &choices,
        )
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
