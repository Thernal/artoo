//! Notifications for the open repository's project, read for the Inbox (`super::inbox`, ⌘⇧N) and the menu's
//! count (#72): what they are and what was seen of them is the requests' (`artoo_requests::notices`); the tab
//! reads them in the background, says what is new to the system (#26) and opens one.

use super::Workspace;
use crate::links::Target;
use artoo_requests::notices::{self, EVERY};
pub(super) use artoo_requests::notices::{Kind, Notice, Notices};
use gpui::{App, Context};

impl Workspace {
    /// Reads the to-dos and the discussions of the user's requests again, in the background.
    pub(super) fn load_notices(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = self.web.clone() else {
            return;
        };
        let host = super::host_of(&remote, cx);
        if !self.requests.read(cx).available {
            self.notices.update(cx, |n, _| n.list.clear());
            return;
        }
        let reading = self.notices.update(cx, |n, _| n.begin(&remote.base));
        let requests = self.requests.read(cx).list.clone();
        let tick = self.requests.read(cx).waiting.tick();
        let task = cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { notices::read(&*host, &requests, reading) })
                .await;
            this.update(cx, |this, cx| {
                if let Some(mut arrived) = this.notices.update(cx, |n, _| n.arrive(found, tick)) {
                    let news = std::mem::take(&mut arrived.news);
                    this.notify_notices(news, cx);
                    this.settle_waiting(cx);
                    let me = this.requests.read(cx).login.clone().unwrap_or_default();
                    let requests = this.requests.clone();
                    this.notices.update(cx, |n, cx| {
                        for iid in n.iids() {
                            n.show(iid, &requests.read(cx).waiting, (&this.me, &me));
                        }
                        n.put(arrived);
                    });
                    if this.inbox {
                        this.inbox_selected(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        });
        self.notices.update(cx, |n, _| n.task = Some(task));
        if self.notices.read(cx).poll.is_none() {
            let poll = cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(EVERY).await;
                    if this.update(cx, |this, cx| this.load_notices(cx)).is_err() {
                        return;
                    }
                }
            });
            self.notices.update(cx, |n, _| n.poll = Some(poll));
        }
    }

    /// The Inbox opened: everything read afresh, not only what reports a change.
    pub(super) fn refresh_notices(&mut self, cx: &mut Context<Self>) {
        self.notices.update(cx, |n, _| n.forget());
        self.load_notices(cx);
    }

    /// Notice `ix` is not read yet.
    pub(super) fn notice_unread(&self, ix: usize, cx: &App) -> bool {
        self.notices.read(cx).unread_at(ix)
    }

    /// Opens notice `ix`: its request, at its comment.
    pub(super) fn open_notice(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(n) = self.notices.update(cx, |n, _| n.open(ix)) else {
            return;
        };
        // On an outdated comment it opens on the diff since it, once that is known (#46).
        self.since_on_open = n.note.map(|note| (n.iid, note));
        // The keyboard goes to what was opened.
        self.set_zone(super::zones::Zone::Diff, cx);
        self.go_to(
            Target::Request {
                iid: n.iid,
                note: n.note,
                line: None,
            },
            cx,
        );
    }

    /// `e`: done with notice `ix` — a to-do is marked done on GitLab, a comment counts as read
    /// from now on.
    pub(super) fn dismiss_notice(&mut self, ix: usize, cx: &mut Context<Self>) {
        let base = self.web.as_ref().map(|w| w.base.clone());
        let Some(n) = self
            .notices
            .update(cx, |ns, _| ns.dismiss(ix, base.as_deref()))
        else {
            return;
        };
        // Gone at once; a refusal puts it back where it was and says why (#139).
        if let Some(id) = n.todo
            && let Some(remote) = self.web.clone()
        {
            let host = super::host_of(&remote, cx);
            cx.spawn(async move |this, cx| {
                let done = cx
                    .background_executor()
                    .spawn(async move { host.mark_todo_done(id) })
                    .await;
                this.update(cx, |this, cx| {
                    if let Err(e) = done {
                        this.notices.update(cx, |ns, _| ns.put_back(ix, n));
                        this.error = Some(format!("Not marked done — {e:#}").into());
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// Everything listed counts as read.
    pub(super) fn read_all_notices(&mut self, cx: &mut Context<Self>) {
        let agents = self.agent_notices().into_iter().map(|(_, _, key)| key);
        self.notices.update(cx, |n, _| n.read_all(agents));
        cx.notify();
    }
}
