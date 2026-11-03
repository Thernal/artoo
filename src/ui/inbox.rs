//! The Inbox, a place of the menu (#49), as Design L4 draws it (STYLE.md: "notifications listed,
//! the selected one previewed and answerable"): the notifications on the left under Today and
//! Earlier; on the right the selected one's request, the code its discussion is on (the lines
//! around it as the request changed them), the whole discussion, and a reply written there —
//! Reply ⌘↵, Resolve. A notice with no discussion (a mention in the request's description, say)
//! is answered with a comment on the request; a review request is only opened. j k move, ↵ opens
//! the request, e is done with it, r replies. Per project: the open repository's notices, and
//! among them what your agents answered on your notes (#116): a question, or "done".

use super::composer::Target;
use super::notifications::{Kind, Notice};
use super::{Workspace, px};
use crate::mr::Thread;
use artoo_agents_api::view::{AnswerEvent, AnswerLook, From, OnAnswer, answer_preview, answer_row};
use artoo_requests::inbox::excerpt;
pub(super) use artoo_requests::inbox::{InboxRow, InboxView};
use artoo_requests::inbox_view::{self, InboxEvent, OnInbox, RowLook};
pub(super) use artoo_ui_kit::format::now;
use artoo_ui_kit::markdown::Selecting;
use gpui::{AnyElement, App, Context, WeakEntity, div, prelude::*};
use std::rc::Rc;

impl Workspace {
    /// What the Inbox lists under its filter, newest first (`InboxView::rows`).
    pub(super) fn inbox_rows(&self, cx: &App) -> Vec<InboxRow> {
        self.inbox_view
            .read(cx)
            .rows(self.notices.read(cx), &self.agent_notices(cx))
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
        self.notices.read(cx).unread_with(&self.agent_notices(cx))
    }

    /// The Inbox's selected row (`InboxView::current`).
    pub(super) fn inbox_current(&self, cx: &App) -> Option<InboxRow> {
        self.inbox_view
            .read(cx)
            .current(self.notices.read(cx), &self.agent_notices(cx))
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
            .inbox_view
            .read(cx)
            .step(self.notices.read(cx), &answers, down);
        if let Some(next) = next {
            self.inbox_select(next, cx);
        }
    }

    fn inbox_select(&mut self, row: InboxRow, cx: &mut Context<Self>) {
        if Some(row) != self.inbox_current(cx) {
            self.drop_inbox_reply();
        }
        match row {
            InboxRow::Notice(ix) => {
                self.inbox_view.update(cx, |v, _| v.agent = None);
                self.notices.update(cx, |n, _| n.at = ix);
                self.inbox_selected(cx);
            }
            InboxRow::Agent(id) => {
                self.inbox_view.update(cx, |v, _| v.agent = Some(id));
                // Looked at: read, until the agent writes again.
                if let Some((_, _, key)) =
                    self.agent_notices(cx).into_iter().find(|(i, ..)| *i == id)
                {
                    self.notices.update(cx, |n, _| n.mark_read(key));
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

    /// The selected notice's discussion, when it has one and it was read.
    fn inbox_thread<'a>(&self, n: &Notice, cx: &'a App) -> Option<&'a Thread> {
        self.notices.read(cx).thread_of(n.iid, n.note?)
    }

    /// The selected notice changed (or its discussions came in): reads the code its discussion
    /// is on, once, in the background.
    pub(super) fn inbox_selected(&mut self, cx: &mut Context<Self>) {
        let notices = self.notices.read(cx);
        let Some(n) = notices.list.get(notices.at) else {
            return;
        };
        let Some(t) = self.inbox_thread(n, cx) else {
            return;
        };
        if self.inbox_view.read(cx).excerpts.contains_key(&t.id) {
            return;
        }
        let (Some(root), Some(path)) = (self.root.clone(), t.path.clone()) else {
            return;
        };
        let Some(m) = self.find_request(n.iid, cx) else {
            return;
        };
        let heads: Vec<String> = t.commit.iter().cloned().chain([m.head.clone()]).collect();
        let (id, target, new, old) = (t.id.clone(), m.target.clone(), t.new_line, t.old_line);
        let reading = id.clone();
        let task = cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { excerpt(&root, &target, &heads, &path, new, old) })
                .await;
            this.update(cx, |this, cx| {
                this.inbox_view.update(cx, |v, _| {
                    v.reading.take_if(|r| *r == id);
                    v.excerpts.insert(id, found);
                });
                cx.notify();
            })
            .ok();
        });
        self.inbox_view.update(cx, |v, _| {
            v.reading = Some(reading);
            v.task = Some(task);
        });
    }

    /// r in the Inbox: the composer under the selected notice. A review request has nothing to
    /// answer.
    pub(super) fn reply_in_inbox(&mut self, cx: &mut Context<Self>) {
        // An agent's question: your answer, in its thread.
        if let Some(InboxRow::Agent(id)) = self.inbox_current(cx) {
            return self.start_answer(id, cx);
        }
        let notices = self.notices.read(cx);
        let Some(n) = notices.list.get(notices.at) else {
            return;
        };
        if matches!(n.kind, Kind::Review | Kind::Todo) {
            return;
        }
        let iid = n.iid;
        let (discussion, spot) = match self.inbox_thread(n, cx) {
            Some(t) => (
                Some(t.id.clone()),
                t.path.clone().zip(t.new_line.or(t.old_line)),
            ),
            None => (None, None),
        };
        self.start_inbox_reply(iid, discussion, spot, cx);
    }

    /// Resolves (or opens again) discussion `id` of request `iid` from the Inbox: at once, faded
    /// until the host answers (#139).
    fn inbox_resolve(&mut self, iid: u64, id: String, resolved: bool, cx: &mut Context<Self>) {
        let op = super::waiting::Op::Resolve {
            thread: id.clone(),
            resolved,
        };
        self.send_change(
            iid,
            vec![op],
            move |host| host.resolve(iid, &id, resolved),
            Self::say_refused,
            cx,
        );
    }

    /// A click in the Inbox (`InboxHost::act`).
    fn inbox_act(&mut self, event: InboxEvent, cx: &mut Context<Self>) {
        match event {
            InboxEvent::Filter(f) => {
                self.inbox_view.update(cx, |v, _| v.filter = f);
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
    pub(super) fn inbox_filters(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let this = cx.weak_entity();
        let on: OnInbox = Rc::new(move |event, cx| {
            this.update(cx, |this, cx| this.inbox_act(event, cx)).ok();
        });
        inbox_view::filters(self.inbox_view.read(cx).filter, &on)
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

/// What the Inbox (`artoo_requests::inbox_page`) asks of the tab: your notes an agent answered, drawn by the agents
/// (`artoo_agents_api::view::answer_row`), the composer, the clicks.
pub(super) struct InboxHost(pub WeakEntity<Workspace>);

impl artoo_requests::inbox_page::InboxHost for InboxHost {
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
