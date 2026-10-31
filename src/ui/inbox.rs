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
use super::{Workspace, format, island, px, theme};
use crate::mr::Thread;
use artoo_agents::inbox::{AnswerEvent, AnswerLook, From, OnAnswer, answer_preview, answer_row};
use artoo_requests::inbox::excerpt;
pub(super) use artoo_requests::inbox::{InboxRow, InboxView};
use artoo_requests::inbox_view::{self, InboxEvent, NoticeLook, OnInbox, RowLook};
pub(super) use artoo_ui_kit::format::now;
use gpui::{AnyElement, Context, div, prelude::*};
use std::rc::Rc;

impl Workspace {
    /// What the Inbox lists under its filter, newest first (`InboxView::rows`).
    pub(super) fn inbox_rows(&self) -> Vec<InboxRow> {
        self.inbox_view.rows(&self.notices, &self.agent_notices())
    }

    /// Your notes an agent answered that wait for you: it asked something (your answer, once
    /// given, waits to be sent from here), or it says it is done. With when it last wrote, and
    /// the key that is read once looked at (a new answer is new again).
    pub(super) fn agent_notices(&self) -> Vec<(u64, i64, String)> {
        use super::agent::Kind as AgentKind;
        let alive = |session: &str| self.chat_listens(session);
        self.comments
            .iter()
            .filter(|c| !c.archived)
            .filter(|c| match super::agent::kind_of(c, &alive) {
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
    pub(super) fn inbox_unread(&self) -> usize {
        self.notices.unread_with(&self.agent_notices())
    }

    /// The Inbox's selected row (`InboxView::current`).
    pub(super) fn inbox_current(&self) -> Option<InboxRow> {
        self.inbox_view
            .current(&self.notices, &self.agent_notices())
    }

    /// Your answer to agent's question `id` was written in the Inbox: it goes to its chat now,
    /// and the next row is selected (Design D: "Answer, next").
    pub(super) fn inbox_answered(&mut self, id: u64, cx: &mut Context<Self>) {
        let rows = self.inbox_rows();
        let at = rows.iter().position(|r| *r == InboxRow::Agent(id));
        self.send_agent(vec![id], cx);
        let rows = self.inbox_rows();
        if let Some(&next) = at.and_then(|a| rows.get(a).or(rows.last())) {
            self.inbox_select(next, cx);
        }
    }

    /// j k in the Inbox: the next or previous listed notice.
    pub(super) fn inbox_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let answers = self.agent_notices();
        if let Some(next) = self.inbox_view.step(&self.notices, &answers, down) {
            self.inbox_select(next, cx);
        }
    }

    fn inbox_select(&mut self, row: InboxRow, cx: &mut Context<Self>) {
        if Some(row) != self.inbox_current() {
            self.drop_inbox_reply();
        }
        match row {
            InboxRow::Notice(ix) => {
                self.inbox_view.agent = None;
                self.notices.at = ix;
                self.inbox_selected(cx);
            }
            InboxRow::Agent(id) => {
                self.inbox_view.agent = Some(id);
                // Looked at: read, until the agent writes again.
                if let Some((_, _, key)) = self.agent_notices().into_iter().find(|(i, ..)| *i == id)
                {
                    self.notices.mark_read(key);
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
        match self.inbox_current() {
            Some(InboxRow::Agent(id)) => self.open_agent_note(id, cx),
            Some(InboxRow::Notice(ix)) => self.open_notice(ix, cx),
            None => {}
        }
    }

    /// e in the Inbox: done with the notice; an agent's question for later (read, the next one
    /// on), its "done" resolved.
    pub(super) fn inbox_done(&mut self, cx: &mut Context<Self>) {
        match self.inbox_current() {
            Some(InboxRow::Agent(id)) => {
                let addressed = self
                    .comments
                    .iter()
                    .any(|c| c.id == id && c.state == crate::review::State::Addressed);
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
            .comments
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
    fn inbox_thread(&self, n: &Notice) -> Option<&Thread> {
        self.notices.thread_of(n.iid, n.note?)
    }

    /// The selected notice changed (or its discussions came in): reads the code its discussion
    /// is on, once, in the background.
    pub(super) fn inbox_selected(&mut self, cx: &mut Context<Self>) {
        let Some(n) = self.notices.list.get(self.notices.at) else {
            return;
        };
        let Some(t) = self.inbox_thread(n) else {
            return;
        };
        if self.inbox_view.excerpts.contains_key(&t.id) {
            return;
        }
        let (Some(root), Some(path)) = (self.root.clone(), t.path.clone()) else {
            return;
        };
        let Some(m) = self.requests.read(cx).find(n.iid) else {
            return;
        };
        let heads: Vec<String> = t.commit.iter().cloned().chain([m.head.clone()]).collect();
        let (id, target, new, old) = (t.id.clone(), m.target.clone(), t.new_line, t.old_line);
        self.inbox_view.reading = Some(id.clone());
        self.inbox_view.task = Some(cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move { excerpt(&root, &target, &heads, &path, new, old) })
                .await;
            this.update(cx, |this, cx| {
                this.inbox_view.reading.take_if(|r| *r == id);
                this.inbox_view.excerpts.insert(id, found);
                cx.notify();
            })
            .ok();
        }));
    }

    /// r in the Inbox: the composer under the selected notice. A review request has nothing to
    /// answer.
    pub(super) fn reply_in_inbox(&mut self, cx: &mut Context<Self>) {
        // An agent's question: your answer, in its thread.
        if let Some(InboxRow::Agent(id)) = self.inbox_current() {
            return self.start_answer(id, cx);
        }
        let Some(n) = self.notices.list.get(self.notices.at) else {
            return;
        };
        if matches!(n.kind, Kind::Review | Kind::Todo) {
            return;
        }
        let iid = n.iid;
        let (discussion, spot) = match self.inbox_thread(n) {
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

    /// How the Inbox's views ask the tab.
    fn inbox_on(&self, cx: &Context<Self>) -> OnInbox {
        let this = cx.weak_entity();
        Rc::new(move |event, cx| {
            this.update(cx, |this, cx| match event {
                InboxEvent::Filter(f) => {
                    this.inbox_view.filter = f;
                    if let Some(&first) = this.inbox_rows().first() {
                        this.inbox_select(first, cx);
                    }
                    cx.notify();
                }
                InboxEvent::Pick(ix) => {
                    if this.compact() {
                        this.inbox = false;
                        return this.open_notice(ix, cx);
                    }
                    this.inbox_select(InboxRow::Notice(ix), cx)
                }
                InboxEvent::Open(ix) => {
                    this.inbox = false;
                    this.open_notice(ix, cx)
                }
                InboxEvent::Done(ix) => this.dismiss_notice(ix, cx),
                InboxEvent::Reply => this.reply_in_inbox(cx),
                InboxEvent::Resolve { iid, id, resolved } => {
                    this.inbox_resolve(iid, id, resolved, cx)
                }
                InboxEvent::Menu { iid, at } => this.request_context(iid, at, cx),
            })
            .ok();
        })
    }

    /// The Inbox's filters, in its page bar (#71).
    pub(super) fn inbox_filters(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        inbox_view::filters(self.inbox_view.filter, &self.inbox_on(cx))
    }

    /// The Inbox: its panes, or rows loading before the first read, or one message when nothing
    /// is listed; a banner over it while the host cannot be read (#72).
    pub(super) fn render_inbox(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rows = self.inbox_rows();
        let body = if !self.notices.loaded && rows.is_empty() {
            inbox_view::loading(self.layout.list)
        } else if rows.is_empty() {
            let others = !self.notices.list.is_empty() || !self.agent_notices().is_empty();
            inbox_view::empty(self.inbox_view.filter, others, &self.inbox_on(cx))
        } else {
            self.render_inbox_panes(&rows, cx).into_any_element()
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(super::gap()))
            .children(artoo_requests::list::banner(
                self.requests.read(cx),
                &super::requests::acts_of(cx.entity().downgrade()),
            ))
            .child(body)
    }

    /// The Inbox's panes: the list, and the selected notice previewed and answerable.
    fn render_inbox_panes(
        &self,
        rows: &[InboxRow],
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let today = format::date(now());
        let mut list: Vec<AnyElement> = Vec::new();
        let mut group = None;
        let agents = self.agent_notices();
        for &row in rows {
            let at = match row {
                InboxRow::Notice(ix) => self.notices.list[ix].created,
                InboxRow::Agent(id) => agents
                    .iter()
                    .find(|(i, ..)| *i == id)
                    .map_or(0, |(_, at, _)| *at),
            };
            let heading = if format::date(at) == today {
                "Today"
            } else {
                "Earlier"
            };
            if group != Some(heading) {
                group = Some(heading);
                list.push(inbox_view::heading(heading));
            }
            list.push(match row {
                InboxRow::Notice(ix) => self.render_inbox_row(ix, cx).into_any_element(),
                InboxRow::Agent(id) => self.render_agent_row(id, cx).into_any_element(),
            });
        }
        let preview = self
            .inbox_current()
            .filter(|r| rows.contains(r))
            .and_then(|r| match r {
                InboxRow::Notice(ix) => self
                    .notices
                    .list
                    .get(ix)
                    .map(|n| self.render_inbox_preview(n, cx).into_any_element()),
                InboxRow::Agent(id) => self.render_agent_preview(id, cx),
            })
            .unwrap_or_else(|| {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme::muted())
                    .child("Choose a notification.")
                    .into_any_element()
            });
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(
                if self.compact() {
                    island().flex_1().min_w_0()
                } else {
                    island()
                        .w(px(super::roots::list_width(self.layout.list)))
                        .flex_none()
                }
                .relative()
                .child(self.widths.measure("inbox"))
                .child(
                    div()
                        .id("inbox-list")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .pb(px(6.))
                        .children(list),
                ),
            )
            // Narrow, the list is the page and a notice opens where it points (#80).
            // Its handle widens or narrows the list, kept for every list page (the developer,
            // 2026-10-23); a double click sets it back.
            .children((!self.compact()).then(|| self.handle(super::Split::List, cx)))
            .children((!self.compact()).then(|| {
                island()
                    .flex_1()
                    .min_w_0()
                    .bg(theme::editor())
                    .child(
                        div()
                            .id("inbox-preview")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .child(preview),
                    )
                    .child(inbox_view::foot_keys(match self.inbox_current() {
                        Some(InboxRow::Agent(id)) => Some(
                            self.comments
                                .iter()
                                .any(|c| c.id == id && c.state == crate::review::State::Addressed),
                        ),
                        _ => None,
                    }))
            }))
    }

    /// One notice's row (`inbox_view::notice_row`).
    fn render_inbox_row(&self, ix: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let look = RowLook {
            fresh: self.notice_unread(ix),
            on: self.inbox_current() == Some(InboxRow::Notice(ix)),
            wide: self.widths.room("inbox").second(),
        };
        inbox_view::notice_row(&self.notices.list[ix], ix, look, &self.inbox_on(cx))
    }

    /// Who an agent's answer is from: the chat's agent ("Claude"), and the chat's title.
    fn agent_of(&self, c: &crate::review::Comment) -> From {
        let title = c
            .sent
            .as_ref()
            .map(|s| crate::labels::chat(&s.session).unwrap_or_else(|| s.title.clone()))
            .unwrap_or_default();
        let agent = c
            .sent
            .as_ref()
            .and_then(|s| self.agent_chats.iter().find(|ch| ch.session == s.session))
            .map(|ch| ch.agent.clone())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| "Agent".to_owned());
        From { agent, chat: title }
    }

    /// How the Inbox's agents' answers ask the tab.
    fn answer_on(&self, cx: &Context<Self>) -> OnAnswer {
        let this = cx.weak_entity();
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

    /// One agent's answer (#116) in the list (`artoo_agents::inbox::answer_row`).
    fn render_agent_row(&self, id: u64, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let Some(c) = self.comments.iter().find(|c| c.id == id) else {
            return div().id(("inbox-agent", id as usize));
        };
        let look = AnswerLook {
            fresh: self
                .agent_notices()
                .iter()
                .any(|(i, _, key)| *i == id && !self.notices.is_read(key)),
            on: self.inbox_current() == Some(InboxRow::Agent(id)),
            wide: self.widths.room("inbox").second(),
        };
        answer_row(c, self.agent_of(c), look, &self.answer_on(cx))
    }

    /// The selected agent's answer, its thread the tab's (`artoo_agents::inbox::answer_preview`).
    fn render_agent_preview(&self, id: u64, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.comments.iter().find(|c| c.id == id)?;
        let thread = self.render_thread(id, 0., cx.entity().downgrade(), cx);
        Some(answer_preview(
            c,
            self.agent_of(c),
            thread.into_any_element(),
            &self.answer_on(cx),
        ))
    }

    /// The selected notice: its request, the code, the discussion, the reply
    /// (`inbox_view::notice_preview`).
    fn render_inbox_preview(&self, n: &Notice, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        // A review request has nothing to answer: no composer is drawn for it.
        let answerable = !matches!(n.kind, Kind::Review | Kind::Todo);
        let writing = (answerable
            && self
                .compose
                .as_ref()
                .is_some_and(|c| c.target == Target::InboxReply))
        .then(|| {
            self.reply_editor(cx.entity().downgrade(), cx)
                .map(|e| e.max_w(px(720.)).into_any_element())
                .unwrap_or_else(|| div().into_any_element())
        });
        let on = self.inbox_on(cx);
        let rq = self.requests.read(cx);
        let look = NoticeLook {
            ix: self.notices.at,
            request: rq.find(n.iid),
            thread: self.inbox_thread(n),
            view: &self.inbox_view,
            waiting: &rq.waiting,
            me: rq.login.as_deref().unwrap_or_default(),
            rich: self.settings.rich_comments,
            selecting: self.selecting(cx.entity().downgrade()),
            writing,
        };
        inbox_view::notice_preview(n, look, &on, cx)
    }
}
