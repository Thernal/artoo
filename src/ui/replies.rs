//! What the composer posts to a merge request: a reply to a discussion, the Inbox's reply, a comment on a line or
//! a pending one, sent with its comment shown faded until the host answers (#139).

use super::Workspace;
use super::composer::{Compose, Target};
use super::typing::Input;
use crate::textedit::TextEdit;
use artoo_requests_api::Change;
use gpui::{App, ClipboardItem, Context};

impl Workspace {
    /// Starts a reply under discussion `ix` of the open merge request.
    pub(super) fn start_reply(&mut self, ix: usize, cx: &mut Context<Self>) {
        // One still on its way to the host has no id to reply to yet (#139).
        let Some(t) = self
            .request_thread(ix, cx)
            .filter(|t| !self.is_temp_thread(&t.id))
        else {
            return;
        };
        // A general discussion has no line: its reply is written in the Discussions window (#199).
        let (path, line) = (
            t.path.clone().unwrap_or_default(),
            t.new_line.or(t.old_line).unwrap_or(0),
        );
        let old = t.new_line.is_none() && t.path.is_some();
        self.compose = Some(Compose {
            path,
            old,
            first: None,
            line,
            code: String::new(),
            edit: TextEdit::default(),
            at: String::new(),
            target: Target::Reply(ix),
            error: None,
            preview: false,
            pick: 0,
            closed: None,
            page: None,
            file: None,
            answers: None,
        });
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        self.thread_at = Some(ix);
        if !self.mr_open(ix, cx) {
            self.toggled_mr.insert(ix);
        }
        self.refresh_rows(cx);
        cx.notify();
    }

    /// The Inbox's composer: a reply to discussion `discussion` of request `iid` (on `path` and
    /// `line` when it is on code), or with no discussion a comment on the request. A comment
    /// being written elsewhere is kept: the Inbox then waits for it.
    pub(super) fn start_inbox_reply(
        &mut self,
        iid: u64,
        discussion: Option<String>,
        spot: Option<(String, u32)>,
        cx: &mut Context<Self>,
    ) {
        if self
            .compose
            .as_ref()
            .is_some_and(|c| c.target != Target::InboxReply)
        {
            return;
        }
        let (path, line) = spot.unwrap_or_default();
        self.compose = Some(Compose {
            path,
            old: false,
            first: None,
            line,
            code: String::new(),
            edit: TextEdit::default(),
            at: String::new(),
            target: Target::InboxReply,
            error: None,
            preview: false,
            pick: 0,
            closed: None,
            page: None,
            file: None,
            answers: None,
        });
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        self.set_inbox_reply(Some((iid, discussion)), cx);
        cx.notify();
    }

    /// Discussions' foot: a comment on the request on screen as a whole, written in the window (#199).
    pub(super) fn start_request_comment(&mut self, cx: &mut Context<Self>) {
        if let Some(iid) = self.current_request(cx) {
            self.start_inbox_reply(iid, None, None, cx);
        }
    }

    /// Posts the Inbox's reply: to its discussion, or as a comment on the request.
    pub(super) fn post_inbox_reply(&mut self, cx: &mut Context<Self>) {
        let (Some(compose), Some((iid, discussion))) =
            (self.compose.as_ref(), self.inbox_reply(cx))
        else {
            return;
        };
        let body = compose.edit.text().trim().to_owned();
        if body.is_empty() {
            return;
        }
        let change = match discussion {
            Some(thread) => Change::Reply { thread, body },
            None => Change::Comment { body },
        };
        self.submit_compose(iid, change, cx);
    }

    /// One's own last note in discussion `ix`, when the request can take changes.
    pub(super) fn my_note_in(&self, ix: usize, cx: &App) -> Option<u64> {
        let login = self.request_login(cx)?;
        self.request_target(cx)?;
        let t = self.request_thread(ix, cx)?;
        t.notes
            .iter()
            .rev()
            .find(|n| n.login == login)
            .map(|n| n.id)
    }

    /// Posts the comment being written to merge request `iid`, on its line.
    /// With `draft`, as a pending comment that only its author sees until the review is submitted.
    pub(super) fn post_comment(&mut self, iid: u64, draft: bool, cx: &mut Context<Self>) {
        let Some(compose) = self.compose.as_ref() else {
            return;
        };
        let body = compose.edit.text().trim().to_owned();
        let head = match self.shown_diff(cx).as_ref().map(|d| &d.header) {
            Some(super::Header::Compare { head, .. }) => head.1.to_string(),
            _ => return,
        };
        // On a page the file is the page's, and its counters come from its hunks (#11).
        let at_file = compose.page.unwrap_or(self.diff_file(cx));
        let diff = self.shown_diff(cx);

        let file = diff.as_ref().and_then(|d| d.files.get(at_file));
        let pair = file
            .filter(|_| compose.page.is_some())
            .map(|f| f.line_pair(compose.old, compose.line));
        let file = file.map(|f| (f.new_path.clone(), f.old_path.clone()));
        if body.is_empty() {
            return;
        }
        let Some((new_path, old_path)) = file else {
            return;
        };
        self.last_request_target = Some(iid);
        let (old_line, new_line) =
            pair.unwrap_or_else(|| self.rv().line_pair(compose.line, compose.old, cx));
        let range = compose.first.and_then(|first| {
            Some((
                self.rv().diff_line(first, compose.old, cx)?,
                self.rv().diff_line(compose.line, compose.old, cx)?,
            ))
        });
        let path = new_path.clone().or(old_path.clone()).unwrap_or_default();
        let old_path = old_path.or(new_path).unwrap_or_default();
        let change = Change::OnLine {
            head,
            path,
            old_path,
            new_line,
            old_line,
            range,
            body,
            draft,
        };
        self.submit_compose(iid, change, cx);
    }

    /// Writes the reply being composed to the discussion it answers.
    pub(super) fn post_reply(&mut self, ix: usize, cx: &mut Context<Self>) {
        let (Some(compose), Some(thread), Some(iid)) = (
            self.compose.as_ref(),
            self.request_thread(ix, cx),
            self.current_request(cx),
        ) else {
            return;
        };
        let (body, id) = (compose.edit.text().trim().to_owned(), thread.id.clone());
        if body.is_empty() || self.is_temp_thread(&id) {
            return;
        }
        self.submit_compose(iid, Change::Reply { thread: id, body }, cx);
    }

    /// Sends what the composer holds, as `change` on request `iid`: the composer closes
    /// and the comment shows at once, faded (#139). Refused, the composer comes back with its text
    /// and the reason when nothing else is being written and its place still shows; else its text
    /// goes to the clipboard and the error bar says why.
    pub(super) fn submit_compose(&mut self, iid: u64, change: Change, cx: &mut Context<Self>) {
        if self.web.is_none() {
            return;
        }
        let Some(mut compose) = self.compose.take() else {
            return;
        };
        let reply = self.inbox_reply(cx);
        self.set_inbox_reply(None, cx);
        // A reply or an edit comes back to its discussion, wherever it moved.
        let thread = compose
            .thread()
            .and_then(|ix| self.request_thread(ix, cx))
            .map(|t| t.id.clone());
        self.refresh_rows(cx);
        self.rv().remeasure_pages(cx);
        let refused = move |this: &mut Self, why: String, cx: &mut Context<Self>| {
            let place = match &thread {
                Some(id) => this.request_threads(cx).iter().position(|t| &t.id == id),
                None => Some(usize::MAX),
            };
            let shows = match compose.target {
                Target::InboxReply => {
                    this.inbox
                        && this.inbox_reply(cx).is_none()
                        && reply.as_ref().is_some_and(|(i, _)| {
                            this.requests_api().and_then(|r| r.selected_notice(cx)) == Some(*i)
                        })
                }
                Target::Request(_) => {
                    this.current_request(cx) == Some(iid)
                        && (compose.page.is_some()
                            || this.current_path(cx).as_deref() == Some(compose.path.as_str()))
                }
                _ => this.current_request(cx) == Some(iid),
            };
            match place {
                Some(ix) if this.compose.is_none() && shows => {
                    if ix != usize::MAX {
                        compose.set_thread(ix);
                    }
                    compose.error = Some(why);
                    if compose.target == Target::InboxReply {
                        this.set_inbox_reply(reply, cx);
                    }
                    this.compose = Some(compose);
                    this.take_keyboard(Input::Compose, cx);
                    this.reset_field_edit();
                    this.refresh_rows(cx);
                    this.rv().remeasure_pages(cx);
                }
                _ => {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        compose.edit.text().to_owned(),
                    ));
                    this.error =
                        Some(format!("Not sent — {why}. Its text is on the clipboard.").into());
                }
            }
            cx.notify();
        };
        self.send_change(iid, change, refused, cx);
    }
}
