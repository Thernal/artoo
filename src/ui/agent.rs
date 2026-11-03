//! Sending the review to an agent's chat (`crate::agents`): the reviewer's comments stay drafts
//! until *Send to agent* (⌥⌘A); a card (`Agents::pick_chat`) then lists this repository's chats — those listening first,
//! then Claude Code's recent ones, which get the comments when they listen — and a new chat. The
//! agent's answers come back every couple of seconds, and the reviewer resolves or reopens each.

use super::Workspace;
use crate::bus::Chat;
use crate::comment::State;
use crate::comment::{Kind, kind_of};
use artoo_agents_api::view::{self, Act, FormEvent, OnForm};
use gpui::{App, Context, WeakEntity};

impl Workspace {
    /// The drafts the next sending takes.
    pub(super) fn agent_drafts(&self, cx: &App) -> Vec<u64> {
        crate::comment::drafts(self.comments(cx))
    }

    /// Opens the card; without drafts, says so.
    pub(super) fn open_agent_pick(&mut self, cx: &mut Context<Self>) {
        let ids = self.agent_drafts(cx);
        self.open_agent_pick_for(ids, cx);
    }

    /// The chat card for `ids`: where to send them.
    pub(super) fn open_agent_pick_for(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        if ids.is_empty() {
            self.error =
                Some("No comments to send — every comment is already with an agent.".into());
            return cx.notify();
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let project = self.project.clone().unwrap_or_else(|| root.clone());
        self.agents.pick_chat(ids, &root, &project, cx);
        cx.notify();
    }

    /// Change…: the card offers the chats that listen; the one picked takes `branch`'s notes.
    fn open_route_pick(&mut self, branch: String, cx: &mut Context<Self>) {
        let now = self.route_of(Some(&branch), cx).map(|c| c.session);
        self.agents.pick_route(branch, now, cx);
        cx.notify();
    }

    /// Send to … instead: a note its gone chat never took goes to its route now.
    fn resend(&mut self, id: u64, cx: &mut Context<Self>) {
        self.change_comment(id, cx, |c| {
            c.state = State::Draft;
            c.sent = None;
        });
        self.send_agent(vec![id], cx);
    }

    /// Whether a chat is here and listening, as the chats are now: to ask while the tab changes.
    pub(super) fn listening(&self, cx: &App) -> impl Fn(&str) -> bool + use<> {
        let on = self.agents.listening(cx);
        move |session| on.contains(session)
    }

    /// Another chat than the page's is changing file `path` (#77). One in a worktree of its own:
    /// the file is changed there (#117: what it really edits). One in this working tree, where
    /// whose edit is whose cannot be told: a note of its on the file was taken and not answered
    /// yet. Its name, letter and hue.
    pub(super) fn changing(&self, path: &str, cx: &App) -> Option<(String, char, gpui::Rgba)> {
        let alive = self.listening(cx);
        let page = self
            .route_of(self.page_branch(cx).as_deref(), cx)
            .map(|c| c.session);
        let (chats, edits) = (self.agents.chats(cx), self.agents.edits(cx));
        let edited = chats
            .iter()
            .filter(|c| Some(&c.session) != page.as_ref())
            .find(|c| {
                edits
                    .get(&c.session)
                    .is_some_and(|paths| paths.contains(path))
            })
            .map(|c| c.session.clone());
        let session = edited.or_else(|| {
            self.comments(cx)
                .iter()
                .filter(|c| c.path == path && !c.archived && kind_of(c, &alive) == Kind::Working)
                .filter_map(|c| c.sent.as_ref())
                .map(|s| s.session.clone())
                .filter(|s| !edits.contains_key(s))
                .find(|s| Some(s) != page.as_ref())
        })?;
        let chat = chats.iter().find(|c| c.session == session)?;
        let (letter, hue) = self.agents.mark(&session, cx);
        let name = if chat.title.is_empty() {
            chat.agent.clone()
        } else {
            chat.title.clone()
        };
        Some((name, letter, hue))
    }

    /// Sends each of `ids` to its route (#77): the chat working on the branch it was written on,
    /// else the project's default; with no chat listening, the card asks.
    pub(super) fn send_agent(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        if ids.is_empty() {
            return;
        }
        self.refresh_chats(cx);
        let mut by_chat: Vec<(Chat, Vec<u64>)> = Vec::new();
        let mut nowhere = Vec::new();
        for id in ids {
            let branch = self
                .comments(cx)
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.branch.clone());
            match self.route_of(branch.as_deref(), cx) {
                Some(chat) => match by_chat.iter_mut().find(|(c, _)| c.session == chat.session) {
                    Some((_, ids)) => ids.push(id),
                    None => by_chat.push((chat, vec![id])),
                },
                None => nowhere.push(id),
            }
        }
        for (chat, ids) in &by_chat {
            self.change_comments(cx, |list| {
                crate::comment::send(list, ids, &chat.session, &chat.title)
            });
        }
        if !by_chat.is_empty() {
            self.save_comments(cx);
            // A Codex session is handed them now, not on the next beat (#18).
            self.codex_beat(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
        if !nowhere.is_empty() {
            self.open_agent_pick_for(nowhere, cx);
        }
    }

    /// The chat a note on `branch` goes to (`Agents::route_of`), with the one picked for it.
    pub(super) fn route_of(&self, branch: Option<&str>, cx: &App) -> Option<Chat> {
        let main = self.project.as_ref().or(self.root.as_ref())?;
        self.agents.route_of(branch, main, cx)
    }

    /// The branch of the page on screen, as a note written there keeps it: a request's source
    /// branch, a branch's own, else the one checked out (#77).
    pub(super) fn page_branch(&self, cx: &App) -> Option<String> {
        if let Some(iid) = self.current_request(cx)
            && let Some(m) = self.find_request(iid, cx)
            && self.nav.space == super::spaces::Space::Requests
        {
            return Some(m.source.clone());
        }
        let list = self.branch_list(cx);
        let shown = match (
            self.nav.space,
            self.branch_ix(cx).and_then(|ix| list.get(ix)),
        ) {
            (super::spaces::Space::Branches, Some(b)) => Some(b),
            _ => list.iter().find(|b| b.is_head),
        }?;
        // A remote branch is the branch it tracks there.
        Some(match shown.kind {
            crate::git::RefKind::Remote => shown
                .name
                .split_once('/')
                .map_or(shown.name.clone(), |(_, n)| n.to_owned()),
            _ => shown.name.clone(),
        })
    }

    /// The reviewer's pick on the agent's form for comment `id`: saved to the drafts at once
    /// (#10 C) — it goes with the next send.
    pub(super) fn pick_answer(&mut self, id: u64, pick: Option<usize>, cx: &mut Context<Self>) {
        let picked = self.change_comment(id, cx, |c| {
            let form = c.replies.last().and_then(|r| r.form.clone());
            let a = c.answer.get_or_insert_with(|| crate::comment::Answered {
                picks: match &form {
                    Some(f) if f.kind == crate::comment::FormKind::Order => {
                        (0..f.choices().len()).collect()
                    }
                    Some(f) if f.kind.many() => f
                        .choices()
                        .iter()
                        .enumerate()
                        .filter(|(_, o)| o.on)
                        .map(|(i, _)| i)
                        .collect(),
                    _ => Vec::new(),
                },
                ..Default::default()
            });
            match (pick, &form) {
                (None, _) => {
                    a.your_call = !a.your_call;
                }
                (Some(i), Some(f)) if f.kind.many() => {
                    a.your_call = false;
                    match a.picks.iter().position(|p| *p == i) {
                        Some(at) => {
                            a.picks.remove(at);
                        }
                        None => a.picks.push(i),
                    }
                    a.picks.sort_unstable();
                }
                (Some(i), _) => {
                    a.your_call = false;
                    a.picks = vec![i];
                }
            }
            c.state = State::Draft;
        });
        if picked.is_none() {
            return;
        }
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Order: moves item `at` of the answer one place up (or down).
    pub(super) fn move_answer(&mut self, id: u64, at: usize, up: bool, cx: &mut Context<Self>) {
        let moved = self.change_comment(id, cx, |c| {
            let n = c
                .replies
                .last()
                .and_then(|r| r.form.as_ref())
                .map_or(0, |f| f.choices().len());
            let a = c.answer.get_or_insert_with(|| crate::comment::Answered {
                picks: (0..n).collect(),
                ..Default::default()
            });
            let to = if up { at.checked_sub(1) } else { Some(at + 1) };
            let to = to.filter(|&t| t < a.picks.len());
            if let Some(to) = to {
                a.picks.swap(at, to);
                c.state = State::Draft;
            }
            to.is_some()
        });
        if moved == Some(true) {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    /// Under the agent's question on comment `c`: its form (`artoo_agents_api::view::form`), with the lines
    /// of the file when it is the one open, for a Line's suggestions.
    pub(super) fn agent_form(
        &self,
        c: &crate::comment::Comment,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> Option<gpui::AnyElement> {
        let data = self.diff_data(cx);
        let open =
            (data.as_deref()).filter(|_| self.current_path(cx).as_deref() == Some(c.path.as_str()));
        let line = |n: u32| {
            let side = open?.side(false);
            (n >= 1 && n <= side.count()).then(|| side.line(n).0.to_owned())
        };
        let on: OnForm = std::rc::Rc::new(move |event, cx| {
            this.update(cx, |this, cx| this.form_event(event, cx)).ok();
        });
        view::form(
            c,
            &self.agent_drafts(cx),
            open.map(|_| &line as &dyn Fn(u32) -> Option<String>),
            on,
            cx,
        )
    }

    /// A click on an agent's form.
    fn form_event(&mut self, event: FormEvent, cx: &mut Context<Self>) {
        match event {
            FormEvent::Pick(id, pick) => self.pick_answer(id, pick, cx),
            FormEvent::Move(id, at, up) => self.move_answer(id, at, up, cx),
            FormEvent::Own(id) => self.start_answer_value(id, cx),
            FormEvent::Note(id) => self.start_answer(id, cx),
            FormEvent::SendOnly(id) => self.send_agent(vec![id], cx),
            FormEvent::SendAll => {
                let ids = self.agent_drafts(cx);
                self.send_agent(ids, cx)
            }
        }
    }

    /// The asked comment the form keys act on: the first one with a form from the top of the
    /// view (#10 C: 1…9 pick, 0 your call, ⇧N a note).
    pub(super) fn acting_agent_form(&self, cx: &App) -> Option<u64> {
        let top = self.rv().local_top(cx);
        self.rv()
            .rows(cx)
            .iter()
            .skip(top)
            .find_map(|row| match row {
                super::rows::Row::Thread(id) => self
                    .comments(cx)
                    .iter()
                    .find(|c| c.id == *id)
                    .filter(|c| matches!(c.state, State::Sent | State::Draft))
                    .filter(|c| c.replies.last().is_some_and(|r| r.form.is_some()))
                    .map(|c| c.id),
                _ => None,
            })
    }

    /// A digit on the acting form: 0 is your call, 1…9 the option drawn with that number (Line:
    /// the suggestion in order). Whether it took the key.
    pub(super) fn answer_by_key(&mut self, digit: usize, cx: &mut Context<Self>) -> bool {
        use crate::comment::FormKind;
        let Some(id) = self.acting_agent_form(cx) else {
            return false;
        };
        if digit == 0 {
            self.pick_answer(id, None, cx);
            return true;
        }
        let Some(f) = self
            .comments(cx)
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.replies.last())
            .and_then(|r| r.form.clone())
        else {
            return false;
        };
        if f.kind == FormKind::Order || digit > f.choices().len() {
            return false;
        }
        self.pick_answer(id, Some(digit - 1), cx);
        true
    }

    /// `ids` sent to chat `session`, picked on the card: "" a new chat, opened in a terminal.
    fn send_to(&mut self, ids: Vec<u64>, session: String, title: String, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.change_comments(cx, |list| {
            crate::comment::send(list, &ids, &session, &title)
        });
        self.save_comments(cx);
        self.codex_beat(cx);
        if session.is_empty() {
            let prompt = "Artoo sent review comments to this chat. Call the artoo list_comments tool, change the code for each comment and answer each with address_comment.";
            if let Err(e) = crate::platform::open_terminal(&root, "claude", prompt) {
                self.error =
                    Some(format!("Could not open a terminal for the new chat: {e}").into());
            }
        }
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Codex has no watcher of its own: its open sessions here listen through the app, and what
    /// was sent to one is handed over (`crate::codex`, #18) — off the window's thread, one beat
    /// at a time; a snapshot leaves Codex alone.
    fn codex_beat(&mut self, cx: &mut Context<Self>) {
        let snapshot = cfg!(feature = "snapshot") && std::env::var_os("ARTOO_SNAPSHOT").is_some();
        if snapshot || self.codex_beat.is_some() {
            return;
        }
        let Some(project) = self.project.clone().or(self.root.clone()) else {
            return;
        };
        let folder = crate::bus::folder(cx);
        self.codex_beat = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .spawn(async move { folder.codex_beat(&project) })
                .await;
            this.update(cx, |this, _| this.codex_beat = None).ok();
        }));
    }

    /// Takes in what the agents answered since the last look (from the 2-second poll).
    pub(super) fn poll_agents(&mut self, cx: &mut Context<Self>) {
        // A row an agent called the reviewer to, once its page is on screen, has been seen (#160);
        // however it was reached, a row, a link, a search.
        if let Some(item) = self.nav_page().and_then(super::nav::Page::item) {
            self.agent_calls.remove(&item);
        }
        // The chats are read on every beat: the stripe shows who listens, a toast who comes and
        // goes (#77).
        let (root, here) = (
            self.project.clone().or(self.root.clone()),
            self.root.clone(),
        );
        let (changed, said) = self.agents.beat(root.as_deref(), here, cx);
        for text in said {
            self.toast_info(text.into(), cx);
        }
        if changed {
            cx.notify();
        }
        self.codex_beat(cx);
        let Some(root) = self.project.clone().or(self.root.clone()) else {
            return;
        };
        // What agents left on requests, and what they ask to show (#150).
        self.agent_events(&root, cx);
        if let Some(before) = self.agents.take_answers(cx) {
            self.refresh_rows(cx);
            self.notify_answers(&before, cx);
            cx.notify();
        }
    }

    /// Reads this repository's chats again (`Agents::refresh`); whether they changed.
    pub(super) fn refresh_chats(&mut self, cx: &mut Context<Self>) -> bool {
        let root = self.project.clone().or(self.root.clone());
        self.agents.refresh(root.as_deref(), cx)
    }

    /// A click in the Agents window (`artoo_agents_api::view::Act`).
    fn agents_act(&mut self, act: Act, cx: &mut Context<Self>) {
        match act {
            Act::Show(id) => self.jump_to(id, cx),
            Act::Send(ids) => self.send_agent(ids, cx),
            Act::PickChat => self.open_agent_pick(cx),
            Act::SendTo {
                ids,
                session,
                title,
            } => self.send_to(ids, session, title, cx),
            Act::Route(branch) => self.open_route_pick(branch, cx),
            Act::Edit(id) => self.start_edit(super::composer::Change::Agent(id), cx),
            Act::Delete(id) => self.delete_draft(id, cx),
            Act::Resend(id) => self.resend(id, cx),
            Act::Withdraw(id) => self.withdraw(id, cx),
            Act::Archive(id, on) => self.set_archived(id, on, cx),
            Act::ArchiveResolved => self.archive_resolved(cx),
            Act::Resolve(id) => self.resolve_agent_comment(id, cx),
            Act::NameChat { session, agent } => {
                self.open_label(super::labeling::Target::Chat { session, agent }, cx)
            }
        }
    }

    /// A draft goes, no question asked; the toast's Undo brings it back (#76).
    pub(super) fn delete_draft(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(comment) = self.comments(cx).iter().find(|c| c.id == id).cloned() else {
            return;
        };
        self.delete_comment(id, cx);
        self.toast_deleted(comment, cx);
    }

    /// A sent one its chat has not taken yet comes back to the drafts.
    fn withdraw(&mut self, id: u64, cx: &mut Context<Self>) {
        let withdrawn = self.change_comment(id, cx, |c| {
            c.state = State::Draft;
            c.sent = None;
        });
        if withdrawn.is_some() {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    fn set_archived(&mut self, id: u64, on: bool, cx: &mut Context<Self>) {
        if self.change_comment(id, cx, |c| c.archived = on).is_some() {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }

    /// Archive resolved: every resolved one goes off the lists and its line.
    fn archive_resolved(&mut self, cx: &mut Context<Self>) {
        self.change_comments(cx, |list| {
            for c in list.iter_mut().filter(|c| c.state == State::Resolved) {
                c.archived = true;
            }
        });
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// The reviewer agrees the agent did it.
    pub(super) fn resolve_agent_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.set_agent_state(id, State::Resolved, cx);
    }

    /// Not done: back to a draft, to be sent again (with what was said so far).
    pub(super) fn reopen_agent_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.set_agent_state(id, State::Draft, cx);
    }

    fn set_agent_state(&mut self, id: u64, state: State, cx: &mut Context<Self>) {
        if self.change_comment(id, cx, |c| c.state = state).is_some() {
            self.save_comments(cx);
            self.refresh_rows(cx);
            cx.notify();
        }
    }
}

/// What the Agents window asks of the tab (`artoo_agents_api::view::AgentsHost`).
pub(super) struct AgentsHost(pub WeakEntity<Workspace>);

impl view::AgentsHost for AgentsHost {
    fn branch(&self, cx: &App) -> Option<String> {
        self.0.upgrade()?.read(cx).page_branch(cx)
    }

    fn rich(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|tab| tab.read(cx).settings.rich_comments)
    }

    fn act(&self, act: Act, cx: &mut App) {
        self.0.update(cx, |this, cx| this.agents_act(act, cx)).ok();
    }
}
