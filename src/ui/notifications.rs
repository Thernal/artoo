//! Notifications for the open repository's project, read for the Inbox (`super::inbox`, ⌘⇧N) and the menu's
//! count (#72): what they are, what was seen of them and reading them are the requests' (`Requests::read_notices`);
//! the tab says what is new to the system (#26) and opens one.

use super::Workspace;
use crate::links::Target;
use artoo_requests_api::News;
use gpui::Context;

impl Workspace {
    /// Reads the to-dos and the discussions of the user's requests again, in the background.
    pub(super) fn load_notices(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.read_notices(&self.me, cx);
        }
    }

    /// The Inbox opened: everything read afresh, not only what reports a change.
    pub(super) fn refresh_notices(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.refresh_notices(&self.me, cx);
        }
    }

    /// The notifications were read: what is new is said to the system; the Inbox, on screen, reads
    /// the code of the one selected.
    pub(super) fn notices_read(&mut self, news: &[News], cx: &mut Context<Self>) {
        self.notify_notices(news, cx);
        if self.inbox
            && let Some(requests) = self.requests_api()
        {
            requests.read_excerpt(cx);
        }
    }

    /// Opens notice `ix`: its request, at its comment.
    pub(super) fn open_notice(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some((iid, note)) = self.requests_api().and_then(|r| r.open_notice(ix, cx)) else {
            return;
        };
        // On an outdated comment it opens on the diff since it, once that is known (#46).
        self.since_on_open = note.map(|note| (iid, note));
        // The keyboard goes to what was opened.
        self.set_zone(super::zones::Zone::Diff, cx);
        self.go_to(
            Target::Request {
                iid,
                note,
                line: None,
            },
            cx,
        );
    }

    /// `e`: done with notice `ix` — a to-do is marked done on GitLab, a comment counts as read
    /// from now on.
    pub(super) fn dismiss_notice(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.dismiss_notice(ix, cx);
        }
        cx.notify();
    }

    /// Everything listed counts as read.
    pub(super) fn read_all_notices(&mut self, cx: &mut Context<Self>) {
        let agents = self
            .agents
            .inbox_answers(cx)
            .into_iter()
            .map(|(_, _, key)| key);
        // The parts' rows too (a plugin's, #196).
        let parts = artoo_shell_api::notices::Listers::of(&self.services)
            .rows(cx)
            .into_iter()
            .map(|r| r.key);
        if let Some(requests) = self.requests_api() {
            requests.read_all_notices(agents.chain(parts).collect(), cx);
        }
        cx.notify();
    }
}
