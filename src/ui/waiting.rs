//! Changes sent to GitLab or GitHub, shown before the host answers (#139). Each change is a list of
//! `Op`s on a request; what shows is what the host last gave with the changes still waiting applied
//! on top, so two in flight, or a read arriving between them, lose neither, and a refusal takes back
//! only its own. New items get temporary ids, kept in their op, so applying it again gives the same
//! ones. While an op applies, its item is faded and says what it waits for where its time goes.
//!
//! The host's answer to a change stamps it with a tick; a read begun at tick `t` holds every change
//! answered before `t`, so when it arrives those stop applying and the rest go on top of it.

use super::Workspace;
pub(super) use artoo_requests::waiting::{At, Base, Me, Op, Waiting, is_temp, is_temp_thread};
use gpui::{App, Context};

impl Workspace {
    /// Sends `ops` on request `iid` with `work`: they show at once, faded; taken, the request's
    /// discussions are read again, quietly; refused, they are taken back and `refused` is told why.
    /// The request is read again either way when it is open, for whatever went through.
    pub(super) fn send_change(
        &mut self,
        iid: u64,
        ops: Vec<Op>,
        work: impl FnOnce(std::sync::Arc<dyn artoo_hosts_api::Host>) -> anyhow::Result<()>
        + Send
        + 'static,
        refused: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(remote) = self.web.clone() else {
            return;
        };
        let host = super::host_of(&remote, cx);
        let key = self
            .requests
            .update(cx, |rq, _| rq.waiting.send(iid, ops, super::inbox::now()));
        self.show_waiting(iid, cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { work(host) })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => this.requests.update(cx, |rq, _| rq.waiting.took(key)),
                    Err(e) => {
                        this.requests.update(cx, |rq, _| rq.waiting.refused(key));
                        this.show_waiting(iid, cx);
                        refused(this, format!("{e:#}"), cx);
                    }
                }
                // The Inbox reads it again on its next look; at once while it is on screen.
                this.notices.stale(iid);
                if this.inbox {
                    this.load_notices(cx);
                }
                if this.requests.read(cx).current == Some(iid) {
                    this.reload_threads(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The error bar says why a change was refused.
    pub(super) fn say_refused(&mut self, why: String, cx: &mut Context<Self>) {
        self.error = Some(why.into());
        cx.notify();
    }

    /// What shows of request `iid` made again: the host's last answer with the changes still
    /// waiting on it. The folds, the keyboard's discussion and pending comment, and a reply being
    /// written follow theirs wherever it moved.
    pub(super) fn show_waiting(&mut self, iid: u64, cx: &mut App) {
        let me = (
            self.me.clone(),
            self.requests.read(cx).login.clone().unwrap_or_default(),
        );
        let me = (me.0.as_str(), me.1.as_str());
        if self.requests.read(cx).current == Some(iid) {
            let r = &self.requests.read(cx);
            let thread_id = |ix: usize| r.threads.get(ix).map(|t| t.id.clone());
            let toggled: Vec<String> = self
                .toggled_mr
                .iter()
                .filter_map(|&ix| thread_id(ix))
                .collect();
            let at = self.thread_at.and_then(thread_id);
            let draft_at = self.draft_at.and_then(|ix| r.drafts.get(ix)).map(|d| d.id);
            let composing = self
                .compose
                .as_ref()
                .and_then(|c| c.thread())
                .and_then(thread_id);
            let (mut threads, mut drafts, mut approvals) = (
                r.base.threads.clone(),
                r.base.drafts.clone(),
                r.base.approvals.clone(),
            );
            r.waiting
                .apply(iid, r.from, me, &mut threads, &mut drafts, &mut approvals);
            let ix_of = |id: &str| threads.iter().position(|t| t.id == id);
            self.toggled_mr = toggled.iter().filter_map(|id| ix_of(id)).collect();
            self.thread_at = at.and_then(|id| ix_of(&id));
            self.draft_at = draft_at.and_then(|id| drafts.iter().position(|d| d.id == id));
            if let (Some(id), Some(c)) = (composing, self.compose.as_mut())
                && let Some(ix) = ix_of(&id)
            {
                c.set_thread(ix);
            }
            self.requests.update(cx, |rq, _| rq.threads = threads);
            self.requests.update(cx, |rq, _| rq.drafts = drafts);
            self.requests.update(cx, |rq, _| rq.approvals = approvals);
            self.refresh_rows(cx);
        }
        self.notices.show(iid, &self.requests.read(cx).waiting, me);
    }

    /// The host's answer for the open request `iid`, from a read begun at tick `from`.
    pub(super) fn take_base(&mut self, iid: u64, from: u64, base: Base, cx: &mut Context<Self>) {
        self.requests.update(cx, |rq, _| rq.take_base(from, base));
        self.base_taken(iid, cx);
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
        let (current, from) = (self.requests.read(cx).current, self.requests.read(cx).from);
        let notices = &self.notices;
        self.requests.update(cx, |rq, _| {
            rq.waiting.settle(|iid, done| {
                (current != Some(iid) || from > done) && notices.holds(iid, done)
            })
        });
    }
}

/// How faded a waiting item is.
pub const FADED: f32 = 0.5;

impl Workspace {
    /// The snapshot checks' changes on the request open: a reply in its first discussion on a line,
    /// the second resolved or opened again, the approval given or taken back, the first pending
    /// comment sent now and the second deleted.
    #[cfg(feature = "snapshot")]
    pub(super) fn snapshot_changes(&mut self, cx: &mut Context<Self>) {
        let on_lines: Vec<usize> = (self.requests.read(cx).threads.iter().enumerate())
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
        let drafts: Vec<u64> = self.requests.read(cx).drafts.iter().map(|d| d.id).collect();
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
