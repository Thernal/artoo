//! The Inbox, a place of the menu (#49), as Design L4 draws it (STYLE.md: "notifications listed,
//! the selected one previewed and answerable"): the notifications on the left under Today and
//! Earlier; on the right the selected one's request, the code its discussion is on (the lines
//! around it as the request changed them), the whole discussion, and a reply written there —
//! Reply ⌘↵, Resolve. A notice with no discussion (a mention in the request's description, say)
//! is answered with a comment on the request; a review request is only opened. j k move, ↵ opens
//! the request, e is done with it, r replies. Per project: the open repository's notices, and
//! among them what your agents answered on your notes (#116): a question, or "done".

use super::composer::Target;
use super::{Workspace, px};
use artoo_agents_api::view::{AnswerEvent, AnswerLook, From, OnAnswer, answer_preview, answer_row};
pub(super) use artoo_requests_api::InboxRow;
use artoo_requests_api::{InboxEvent, OnInbox, RowLook};
use artoo_ui_kit::markdown::Selecting;
use gpui::{AnyElement, App, Context, WeakEntity, div, prelude::*};
use std::rc::Rc;

impl Workspace {
    /// What the Inbox lists under its filter, newest first (`Requests::inbox_rows`).
    pub(super) fn inbox_rows(&self, cx: &App) -> Vec<InboxRow> {
        let answers = self.agent_notices(cx);
        self.requests_api()
            .map(|r| r.inbox_rows(&answers, cx))
            .unwrap_or_default()
    }

    /// Your notes an agent answered that wait for you: it asked something (your answer, once
    /// given, waits to be sent from here), or it says it is done. With when it last wrote, and
    /// the key that is read once looked at (a new answer is new again).
    pub(super) fn agent_notices(&self, cx: &App) -> Vec<(u64, i64, String)> {
        use crate::comment::{Kind as AgentKind, kind_of};
        let alive = self.listening(cx);
        self.comments(cx)
            .iter()
            .filter(|c| !c.archived)
            .filter(|c| match kind_of(c, &alive) {
                AgentKind::Asks | AgentKind::Addressed => true,
                AgentKind::Draft => c.answer.is_some() && !c.replies.is_empty(),
                _ => false,
            })
            .filter_map(|c| {
                let at = c.replies.last()?.at;
                Some((c.id, at, format!("agent:{}:{}", c.id, c.replies.len())))
            })
            .collect()
    }

    /// The Inbox's count: notices not read, and agents' answers not looked at.
    pub(super) fn inbox_unread(&self, cx: &App) -> usize {
        let answers = self.agent_notices(cx);
        self.requests_api()
            .map_or(0, |r| r.unread_with(&answers, cx))
    }

    /// The Inbox's selected row (`Requests::inbox_current`).
    pub(super) fn inbox_current(&self, cx: &App) -> Option<InboxRow> {
        let answers = self.agent_notices(cx);
        self.requests_api()?.inbox_current(&answers, cx)
    }

    /// What the Inbox's composer writes to: a request, and its discussion; it set.
    pub(super) fn inbox_reply(&self, cx: &App) -> Option<(u64, Option<String>)> {
        self.requests_api()?.inbox_reply(cx)
    }

    pub(super) fn set_inbox_reply(&self, reply: Option<(u64, Option<String>)>, cx: &mut App) {
        if let Some(requests) = self.requests_api() {
            requests.set_inbox_reply(reply, cx);
        }
    }

    /// Your answer to agent's question `id` was written in the Inbox: it goes to its chat now,
    /// and the next row is selected (Design D: "Answer, next").
    pub(super) fn inbox_answered(&mut self, id: u64, cx: &mut Context<Self>) {
        let rows = self.inbox_rows(cx);
        let at = rows.iter().position(|r| *r == InboxRow::Agent(id));
        self.send_agent(vec![id], cx);
        let rows = self.inbox_rows(cx);
        if let Some(&next) = at.and_then(|a| rows.get(a).or(rows.last())) {
            self.inbox_select(next, cx);
        }
    }

    /// j k in the Inbox: the next or previous listed notice.
    pub(super) fn inbox_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let answers = self.agent_notices(cx);
        let next = self
            .requests_api()
            .and_then(|r| r.inbox_step(&answers, down, cx));
        if let Some(next) = next {
            self.inbox_select(next, cx);
        }
    }

    fn inbox_select(&mut self, row: InboxRow, cx: &mut Context<Self>) {
        if Some(row) != self.inbox_current(cx) {
            self.drop_inbox_reply();
        }
        let Some(requests) = self.requests_api() else {
            return;
        };
        requests.inbox_pick(Some(row), cx);
        match row {
            // The code its discussion is on, read once.
            InboxRow::Notice(_) => requests.read_excerpt(cx),
            InboxRow::Agent(id) => {
                // Looked at: read, until the agent writes again.
                if let Some((_, _, key)) =
                    self.agent_notices(cx).into_iter().find(|(i, ..)| *i == id)
                {
                    requests.mark_read(key, cx);
                }
            }
        }
        cx.notify();
    }

    /// An empty reply goes when another notice is chosen; one with text stays until sent or
    /// cancelled.
    fn drop_inbox_reply(&mut self) {
        if self.compose.as_ref().is_some_and(|c| {
            matches!(c.target, Target::InboxReply | Target::Answer(_))
                && c.edit.text().trim().is_empty()
        }) {
            self.compose = None;
        }
    }

    /// ↵ in the Inbox: the notice's request, or the line of the note the agent answered.
    pub(super) fn inbox_open(&mut self, cx: &mut Context<Self>) {
        match self.inbox_current(cx) {
            Some(InboxRow::Agent(id)) => self.open_agent_note(id, cx),
            Some(InboxRow::Notice(ix)) => self.open_notice(ix, cx),
            None => {}
        }
    }

    /// e in the Inbox: done with the notice; an agent's question for later (read, the next one
    /// on), its "done" resolved.
    pub(super) fn inbox_done(&mut self, cx: &mut Context<Self>) {
        match self.inbox_current(cx) {
            Some(InboxRow::Agent(id)) => {
                let addressed = self
                    .comments(cx)
                    .iter()
                    .any(|c| c.id == id && c.state == crate::comment::State::Addressed);
                if addressed {
                    self.resolve_agent_comment(id, cx);
                }
                self.inbox_step(true, cx);
            }
            Some(InboxRow::Notice(ix)) => self.dismiss_notice(ix, cx),
            None => {}
        }
    }

    /// The note an agent answered, where it is written: a file's line in Code, a folder's page;
    /// a line of the old side where a diff shows it.
    fn open_agent_note(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some((path, line, old)) = self
            .comments(cx)
            .iter()
            .find(|c| c.id == id)
            .map(|c| (c.path.clone(), c.line, c.old))
        else {
            return;
        };
        if path.ends_with('/') || old || self.root.is_none() {
            self.inbox = false;
            return self.jump_to(id, cx);
        }
        self.file_jump = Some((path.clone(), line));
        self.nav.space = super::spaces::Space::Code;
        self.open_code_file(path, false, cx);
    }

    /// r in the Inbox: the composer under the selected notice. A review request has nothing to
    /// answer.
    pub(super) fn reply_in_inbox(&mut self, cx: &mut Context<Self>) {
        // An agent's question: your answer, in its thread.
        if let Some(InboxRow::Agent(id)) = self.inbox_current(cx) {
            return self.start_answer(id, cx);
        }
        let Some((iid, discussion, spot)) = self.requests_api().and_then(|r| r.inbox_reply_to(cx))
        else {
            return;
        };
        self.start_inbox_reply(iid, discussion, spot, cx);
    }

    /// Resolves (or opens again) discussion `id` of request `iid` from the Inbox: at once, faded
    /// until the host answers (#139).
    fn inbox_resolve(&mut self, iid: u64, id: String, resolved: bool, cx: &mut Context<Self>) {
        let change = artoo_requests_api::Change::Resolve {
            thread: id,
            resolved,
        };
        self.send_change(iid, change, Self::say_refused, cx);
    }

    /// A click in the Inbox (`InboxHost::act`).
    fn inbox_act(&mut self, event: InboxEvent, cx: &mut Context<Self>) {
        match event {
            InboxEvent::Filter(f) => {
                if let Some(requests) = self.requests_api() {
                    requests.set_inbox_filter(f, cx);
                }
                if let Some(&first) = self.inbox_rows(cx).first() {
                    self.inbox_select(first, cx);
                }
                cx.notify();
            }
            InboxEvent::Pick(ix) => {
                if self.compact() {
                    self.inbox = false;
                    return self.open_notice(ix, cx);
                }
                self.inbox_select(InboxRow::Notice(ix), cx)
            }
            InboxEvent::Open(ix) => {
                self.inbox = false;
                self.open_notice(ix, cx)
            }
            InboxEvent::Done(ix) => self.dismiss_notice(ix, cx),
            InboxEvent::Reply => self.reply_in_inbox(cx),
            InboxEvent::Resolve { iid, id, resolved } => self.inbox_resolve(iid, id, resolved, cx),
            InboxEvent::Menu { iid, at } => self.request_context(iid, at, cx),
        }
    }

    /// The Inbox's filters, in its page bar (#71).
    pub(super) fn inbox_filters(&self, cx: &mut Context<Self>) -> AnyElement {
        let this = cx.weak_entity();
        let on: OnInbox = Rc::new(move |event, cx| {
            this.update(cx, |this, cx| this.inbox_act(event, cx)).ok();
        });
        self.requests_api()
            .map(|r| r.inbox_filters(on, cx))
            .unwrap_or_else(|| div().into_any_element())
    }

    /// Who an agent's answer is from: the chat's agent ("Claude"), and the chat's title.
    fn agent_of(&self, c: &crate::comment::Comment, cx: &App) -> From {
        let title = c
            .sent
            .as_ref()
            .map(|s| crate::labels::chat(&s.session).unwrap_or_else(|| s.title.clone()))
            .unwrap_or_default();
        let agent = c
            .sent
            .as_ref()
            .and_then(|s| {
                self.agents
                    .chats(cx)
                    .iter()
                    .find(|ch| ch.session == s.session)
                    .map(|ch| ch.agent.clone())
            })
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| "Agent".to_owned());
        From { agent, chat: title }
    }

    /// How the Inbox's agents' answers ask the tab.
    fn answer_on(this: WeakEntity<Self>) -> OnAnswer {
        Rc::new(move |event, cx| {
            this.update(cx, |this, cx| match event {
                AnswerEvent::Pick(id) => {
                    if this.compact() {
                        return this.open_agent_note(id, cx);
                    }
                    this.inbox_select(InboxRow::Agent(id), cx)
                }
                AnswerEvent::Open(id) => this.open_agent_note(id, cx),
                AnswerEvent::Done => this.inbox_done(cx),
            })
            .ok();
        })
    }
}

/// What the Inbox (the requests' page) asks of the tab: your notes an agent answered, drawn by the agents
/// (`artoo_agents_api::view::answer_row`), the composer, the clicks.
pub(super) struct InboxHost(pub WeakEntity<Workspace>);

impl artoo_requests_api::InboxHost for InboxHost {
    fn answers(&self, cx: &App) -> Vec<(u64, i64, String)> {
        self.0
            .upgrade()
            .map(|tab| tab.read(cx).agent_notices(cx))
            .unwrap_or_default()
    }

    fn answer_row(&self, id: u64, look: RowLook, cx: &App) -> AnyElement {
        let Some(tab) = self.0.upgrade() else {
            return div().into_any_element();
        };
        let this = tab.read(cx);
        let Some(c) = this.comments(cx).iter().find(|c| c.id == id) else {
            return div().id(("inbox-agent", id as usize)).into_any_element();
        };
        let look = AnswerLook {
            fresh: look.fresh,
            on: look.on,
            wide: look.wide,
        };
        let on = Workspace::answer_on(self.0.clone());
        answer_row(c, this.agent_of(c, cx), look, &on, cx).into_any_element()
    }

    fn answer_preview(&self, id: u64, cx: &App) -> Option<AnyElement> {
        let this = self.0.upgrade()?.read(cx);
        let c = this.comments(cx).iter().find(|c| c.id == id)?;
        let thread = this.render_thread(id, 0., self.0.clone(), cx);
        let on = Workspace::answer_on(self.0.clone());
        Some(answer_preview(c, this.agent_of(c, cx), thread, &on, cx))
    }

    fn answer_done(&self, id: u64, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|tab| {
            tab.read(cx)
                .comments(cx)
                .iter()
                .any(|c| c.id == id && c.state == crate::comment::State::Addressed)
        })
    }

    fn writing(&self, cx: &App) -> Option<AnyElement> {
        let this = self.0.upgrade()?.read(cx);
        this.compose
            .as_ref()
            .is_some_and(|c| c.target == Target::InboxReply)
            .then(|| {
                this.reply_editor(self.0.clone(), cx)
                    .map(|e| e.max_w(px(720.)).into_any_element())
                    .unwrap_or_else(|| div().into_any_element())
            })
    }

    fn selecting(&self, cx: &App) -> Option<Selecting> {
        Some(self.0.upgrade()?.read(cx).selecting(self.0.clone()))
    }

    fn rich(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|tab| tab.read(cx).settings.rich_comments)
    }

    fn narrow(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|tab| tab.read(cx).compact())
    }

    fn act(&self, event: InboxEvent, cx: &mut App) {
        self.0.update(cx, |this, cx| this.inbox_act(event, cx)).ok();
    }
}
