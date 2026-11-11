//! Changes sent to GitLab or GitHub, shown before the host answers (#139). Each change is a list of
//! `Op`s on a request; what shows is what the host last gave with the changes still waiting applied
//! on top, so two in flight, or a read arriving between them, lose neither, and a refusal takes back
//! only its own. New items get temporary ids, kept in their op, so applying it again gives the same
//! ones. While an op applies, its item is faded and says what it waits for where its time goes.
//!
//! The host's answer to a change stamps it with a tick; a read begun at tick `t` holds every change
//! answered before `t`, so when it arrives those stop applying and the rest go on top of it.

use super::Workspace;
use artoo_requests_api::{Answered, Change};
use gpui::{App, Context};

impl Workspace {
    /// Sends `change` on request `iid` (`Requests::send`): it shows at once, faded; taken, the
    /// request's discussions are read again, quietly; refused, it is taken back and `refused` is
    /// told why. The request is read again either way when it is open, for whatever went through.
    pub(super) fn send_change(
        &mut self,
        iid: u64,
        change: Change,
        refused: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        // Touched: its open row stays (#194).
        self.nav.touched(iid);
        if requests.send(iid, change, self.answered(iid, refused, cx), cx) {
            self.show_waiting(iid, cx);
            cx.notify();
        }
    }

    /// What follows a change on request `iid` once its host answered: refused, what shows is made
    /// again and `refused` told why (unless the requests said it); the Inbox, while on screen, and
    /// the request, while open, read again.
    pub(super) fn answered(
        &self,
        iid: u64,
        refused: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
        cx: &Context<Self>,
    ) -> Answered {
        let this = cx.weak_entity();
        Box::new(move |result, cx| {
            this.update(cx, |this, cx| {
                if let Err(why) = result {
                    this.show_waiting(iid, cx);
                    if let Some(why) = why {
                        refused(this, why, cx);
                    }
                }
                if this.inbox {
                    this.load_notices(cx);
                }
                if this.current_request(cx) == Some(iid) {
                    this.reload_threads(cx);
                }
                cx.notify();
            })
            .ok();
        })
    }

    /// The error bar says why a change was refused.
    pub(super) fn say_refused(&mut self, why: String, cx: &mut Context<Self>) {
        self.error = Some(why.into());
        cx.notify();
    }

    /// What shows of request `iid` made again (`Requests::show_waiting`): the host's last answer
    /// with the changes still waiting on it. The folds, the keyboard's discussion and pending
    /// comment, and a reply being written follow theirs wherever it moved.
    pub(super) fn show_waiting(&mut self, iid: u64, cx: &mut App) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        let Some(moved) = requests.show_waiting(iid, &self.me, cx) else {
            return;
        };
        let thread = |ix: usize| moved.threads.get(ix).copied().flatten();
        self.toggled_mr = self
            .toggled_mr
            .iter()
            .filter_map(|&ix| thread(ix))
            .collect();
        self.thread_at = self.thread_at.and_then(thread);
        self.draft_at = (self.draft_at).and_then(|ix| moved.drafts.get(ix).copied().flatten());
        if let Some(c) = self.compose.as_mut()
            && let Some(ix) = c.thread().and_then(thread)
        {
            c.set_thread(ix);
        }
        self.refresh_rows(cx);
    }

    /// What shows of the open request `iid` is made again on the host's answer just taken, and
    /// the changes every reader holds are let go.
    pub(super) fn base_taken(&mut self, iid: u64, cx: &mut Context<Self>) {
        self.settle_waiting(cx);
        self.show_waiting(iid, cx);
        // Both read the discussions just taken in.
        self.compute_histories(cx);
        self.load_awards(cx);
    }

    /// The changes the request open and the Inbox both hold now are let go.
    pub(super) fn settle_waiting(&mut self, cx: &mut App) {
        if let Some(requests) = self.requests_api() {
            requests.settle_waiting(cx);
        }
    }

    /// An id given to a note or a pending comment not on the host yet (#139).
    pub(super) fn is_temp(&self, id: u64) -> bool {
        self.requests_api().is_some_and(|r| r.is_temp(id))
    }

    /// An id given to a discussion not on the host yet: it has no id to reply to.
    pub(super) fn is_temp_thread(&self, id: &str) -> bool {
        self.requests_api().is_some_and(|r| r.is_temp_thread(id))
    }
}

impl Workspace {
    /// The snapshot checks' changes on the request open: a reply in its first discussion on a line,
    /// the second resolved or opened again, the approval given or taken back, the first pending
    /// comment sent now and the second deleted.
    #[cfg(feature = "snapshot")]
    pub(super) fn snapshot_changes(&mut self, cx: &mut Context<Self>) {
        let on_lines: Vec<usize> = (self.request_threads(cx).iter().enumerate())
            .filter(|(_, t)| t.path.is_some() && t.resolvable)
            .map(|(ix, _)| ix)
            .collect();
        if let Some(&ix) = on_lines.first() {
            self.start_reply(ix, cx);
            if let Some(c) = self.compose.as_mut() {
                c.edit = crate::textedit::TextEdit::new("Moved it into the next push.");
            }
            self.add_comment(cx);
        }
        if let Some(&ix) = on_lines.get(1) {
            self.toggle_resolved(ix, cx);
        }
        self.toggle_approval(cx);
        // The first pending comment sent now, the second deleted.
        let drafts: Vec<u64> = self.request_drafts(cx).iter().map(|d| d.id).collect();
        if let Some(&id) = drafts.first() {
            self.send_draft_now(id, cx);
        }
        if let Some(&id) = drafts.get(1) {
            self.discard_draft(id, cx);
        }
        // Where they show: on Changes, at the first.
        self.show_section(super::nav::Section::Changes, cx);
        self.thread_at = None;
        self.step_thread(true, cx);
    }
}
