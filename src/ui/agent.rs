//! Sending the review to an agent's chat (`crate::agents`): the reviewer's comments stay drafts
//! until *Send to agent* (⌥⌘A); a card (`Agents::pick_chat`) then lists this repository's chats — those listening first,
//! then Claude Code's recent ones, which get the comments when they listen — and a new chat. The
//! agent's answers come back every couple of seconds, and the reviewer resolves or reopens each. What is done to a
//! comment — a pick on its form, a send to its route, an archive — is the agents' (`Agents`); the tab hears it changed
//! (`on_comments`) and asks it from what it coordinates: the page's branch, the form under the line, the card.

use super::Workspace;
use crate::bus::Chat;
use crate::comment::State;
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
        self.agents.withdraw(id, cx);
        self.send_agent(vec![id], cx);
    }

    /// Another chat than the page's is changing file `path` (#77, `Agents::changing`): its name, letter and hue.
    pub(super) fn changing(&self, path: &str, cx: &App) -> Option<(String, char, gpui::Rgba)> {
        let page = self
            .route_of(self.page_branch(cx).as_deref(), cx)
            .map(|c| c.session);
        self.agents.changing(path, page.as_deref(), cx)
    }

    /// Sends each of `ids` to its route (#77, `Agents::send`): the chat working on the branch it was written on,
    /// else the project's default; with no chat listening, the card asks.
    pub(super) fn send_agent(&mut self, ids: Vec<u64>, cx: &mut Context<Self>) {
        let Some(main) = self.project.clone().or(self.root.clone()) else {
            return;
        };
        let nowhere = self.agents.send(ids, &main, cx);
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
            && self.nav.space == super::spaces::REQUESTS
        {
            return Some(m.source.clone());
        }
        let list = self.branch_list(cx);
        let shown = match (
            self.nav.space,
            self.branch_ix(cx).and_then(|ix| list.get(ix)),
        ) {
            (super::spaces::BRANCHES, Some(b)) => Some(b),
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
            FormEvent::Pick(id, pick) => self.agents.pick_answer(id, pick, cx),
            FormEvent::Move(id, at, up) => self.agents.move_answer(id, at, up, cx),
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

    /// A digit on the acting form (`Agents::answer_by_key`). Whether it took the key.
    pub(super) fn answer_by_key(&mut self, digit: usize, cx: &mut Context<Self>) -> bool {
        self.acting_agent_form(cx)
            .is_some_and(|id| self.agents.answer_by_key(id, digit, cx))
    }

    /// `ids` sent to chat `session`, picked on the card: "" a new chat, opened in a terminal (`Agents::send_to`).
    fn send_to(&mut self, ids: Vec<u64>, session: String, title: String, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let main = self.project.clone().unwrap_or_else(|| root.clone());
        if let Some(e) = self.agents.send_to(ids, &session, &title, &root, &main, cx) {
            self.error = Some(e.into());
        }
        cx.notify();
    }

    /// Takes in what the agents answered since the last look (from the 2-second poll).
    pub(super) fn poll_agents(&mut self, cx: &mut Context<Self>) {
        // A row an agent called the reviewer to, once its page is on screen, has been seen (#160);
        // however it was reached, a row, a link, a search.
        if let Some(item) = self.nav_page().and_then(|p| self.item_of(p)) {
            self.agent_calls.remove(&item);
        }
        // The chats are read on every beat: the stripe shows who listens, a toast who comes and
        // goes (#77).
        let (root, here) = (
            self.project.clone().or(self.root.clone()),
            self.root.clone(),
        );
        // The Dock counts what waits in every tab's Inbox (#196).
        self.dock_count(cx);
        let (changed, said) = self.agents.beat(root.as_deref(), here, cx);
        for text in said {
            let notice =
                artoo_shell_api::Toast::new(text).kind(artoo_agents_api::notices::PRESENCE);
            self.show_toast(notice, cx);
        }
        if changed {
            cx.notify();
        }
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
            // The agents' own: their window does them.
            Act::Withdraw(_) | Act::Archive(..) | Act::ArchiveResolved | Act::Resolve(_) => {}
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

    /// The reviewer agrees the agent did it.
    pub(super) fn resolve_agent_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.agents.set_state(id, State::Resolved, cx);
    }

    /// Not done: back to a draft, to be sent again (with what was said so far).
    pub(super) fn reopen_agent_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.agents.set_state(id, State::Draft, cx);
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
