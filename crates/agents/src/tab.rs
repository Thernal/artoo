//! A tab's agents implemented (#165 stage 4, `artoo_agents_api::tab`): its chats, its comments to them, the notes,
//! the card asking which chat and Claude Code being connected, each an entity its parts draw.

use crate::agents::Bus;
use crate::chats::Chats;
use crate::comments::AgentComments;
use crate::connect::Connect;
use crate::notes::AgentNotes;
use crate::pick::SendCard;
use artoo_agents_api::bus::Chat;
use artoo_agents_api::comment::{self, Comment, Kind, State};
use artoo_agents_api::notes::{Notes, Shown};
use artoo_agents_api::tab::{Agents, OnChange};
use gpui::{App, AppContext, Entity, Rgba};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// One tab's agents.
pub(crate) struct TabAgents {
    pub chats: Entity<Chats>,
    pub comments: Entity<AgentComments>,
    pub notes: Entity<AgentNotes>,
    pub card: Entity<SendCard>,
    pub connect: Entity<Connect>,
}

/// A new tab's.
pub(crate) fn make(cx: &mut App) -> Rc<dyn Agents> {
    let chats = cx.new(|_| Chats::default());
    Rc::new(TabAgents {
        card: cx.new(|_| SendCard::new(chats.clone())),
        chats,
        comments: cx.new(|_| AgentComments::default()),
        notes: cx.new(|_| AgentNotes::default()),
        connect: cx.new(|_| Connect::default()),
    })
}

impl TabAgents {
    /// The comments changed by `f` when it says so: kept, and the tab told (`on_comments`). What `f` said.
    fn change(&self, cx: &mut App, f: impl FnOnce(&mut AgentComments) -> bool) -> bool {
        crate::comments::change(&self.comments, cx, f)
    }
}

/// The entities behind a tab's `agents`.
pub(crate) fn of(agents: &Rc<dyn Agents>) -> Option<&TabAgents> {
    agents.as_any().downcast_ref()
}

impl Agents for TabAgents {
    fn chats<'a>(&self, cx: &'a App) -> &'a [Chat] {
        &self.chats.read(cx).list
    }

    fn listening(&self, cx: &App) -> HashSet<String> {
        self.chats.read(cx).listening()
    }

    fn mark(&self, session: &str, cx: &App) -> (char, Rgba) {
        self.chats.read(cx).mark(session)
    }

    fn edits<'a>(&self, cx: &'a App) -> &'a HashMap<String, HashSet<String>> {
        &self.chats.read(cx).edits
    }

    fn route_of(&self, branch: Option<&str>, main: &Path, cx: &App) -> Option<Chat> {
        self.chats.read(cx).route_of(branch, main).cloned()
    }

    fn refresh(&self, root: Option<&Path>, cx: &mut App) -> bool {
        let comments = self.comments.clone();
        self.chats
            .update(cx, |c, cx| c.refresh(root, &comments.read(cx).list))
    }

    fn beat(
        &self,
        root: Option<&Path>,
        here: Option<PathBuf>,
        cx: &mut App,
    ) -> (bool, Vec<String>) {
        let comments = self.comments.read(cx).list.clone();
        let said = self
            .chats
            .update(cx, |c, cx| c.beat(root, here, &comments, cx));
        if let Some(root) = root {
            self.chats
                .update(cx, |c, cx| c.beat_providers(root.to_owned(), cx));
        }
        said
    }

    fn comments<'a>(&self, cx: &'a App) -> &'a [Comment] {
        &self.comments.read(cx).list
    }

    fn change_comments(&self, change: &mut dyn FnMut(&mut Vec<Comment>), cx: &mut App) {
        self.comments.update(cx, |c, _| change(&mut c.list));
    }

    fn load_comments(&self, root: PathBuf, cx: &mut App) {
        self.comments.update(cx, |c, _| c.load(root));
    }

    fn save_comments(&self, cx: &App) {
        self.comments.read(cx).save();
    }

    fn pick_answer(&self, id: u64, pick: Option<usize>, cx: &mut App) {
        self.change(cx, |c| c.pick(id, pick));
    }

    fn move_answer(&self, id: u64, at: usize, up: bool, cx: &mut App) {
        self.change(cx, |c| c.move_pick(id, at, up));
    }

    fn answer_by_key(&self, id: u64, digit: usize, cx: &mut App) -> bool {
        self.change(cx, |c| c.key_pick(id, digit))
    }

    fn set_state(&self, id: u64, state: State, cx: &mut App) {
        self.change(cx, |c| c.change(id, |c| c.state = state));
    }

    fn withdraw(&self, id: u64, cx: &mut App) {
        self.change(cx, |c| c.withdraw(id));
    }

    fn archive(&self, id: u64, on: bool, cx: &mut App) {
        self.change(cx, |c| c.change(id, |c| c.archived = on));
    }

    fn archive_resolved(&self, cx: &mut App) {
        self.change(cx, |c| {
            c.archive_resolved();
            true
        });
    }

    fn send(&self, ids: Vec<u64>, main: &Path, cx: &mut App) -> Vec<u64> {
        if ids.is_empty() {
            return ids;
        }
        self.refresh(Some(main), cx);
        let mut by_chat: Vec<(Chat, Vec<u64>)> = Vec::new();
        let mut nowhere = Vec::new();
        for id in ids {
            let branch = (self.comments.read(cx).list.iter())
                .find(|c| c.id == id)
                .and_then(|c| c.branch.clone());
            match self.route_of(branch.as_deref(), main, cx) {
                Some(chat) => match by_chat.iter_mut().find(|(c, _)| c.session == chat.session) {
                    Some((_, ids)) => ids.push(id),
                    None => by_chat.push((chat, vec![id])),
                },
                None => nowhere.push(id),
            }
        }
        if !by_chat.is_empty() {
            self.change(cx, |c| {
                for (chat, ids) in &by_chat {
                    comment::send(&mut c.list, ids, &chat.session, &chat.title);
                }
                true
            });
            // A session the app watches for (Codex's) is handed them now, not on the next beat (#18).
            self.chats
                .update(cx, |c, cx| c.beat_providers(main.to_owned(), cx));
        }
        nowhere
    }

    fn send_to(
        &self,
        ids: Vec<u64>,
        session: &str,
        title: &str,
        root: &Path,
        main: &Path,
        cx: &mut App,
    ) -> Option<String> {
        self.change(cx, |c| {
            comment::send(&mut c.list, &ids, session, title);
            true
        });
        self.chats
            .update(cx, |c, cx| c.beat_providers(main.to_owned(), cx));
        // A new chat: the first agent that starts one (Claude Code's `claude`).
        let program = (artoo_agents_api::provider::all(cx).iter()).find_map(|p| p.program());
        let (true, Some(program)) = (session.is_empty(), program) else {
            return None;
        };
        let prompt = "Artoo sent review comments to this chat. Call the artoo list_comments tool, change the code for each comment and answer each with address_comment.";
        artoo_core::platform::open_terminal(root, program, prompt)
            .err()
            .map(|e| format!("Could not open a terminal for the new chat: {e}"))
    }

    fn changing(&self, path: &str, page: Option<&str>, cx: &App) -> Option<(String, char, Rgba)> {
        let chats = self.chats.read(cx);
        let alive = |session: &str| chats.listens(session);
        let edited = (chats.list.iter())
            .filter(|c| Some(c.session.as_str()) != page)
            .find(|c| (chats.edits.get(&c.session)).is_some_and(|paths| paths.contains(path)))
            .map(|c| c.session.clone());
        let session = edited.or_else(|| {
            (self.comments.read(cx).list.iter())
                .filter(|c| c.path == path && !c.archived)
                .filter(|c| comment::kind_of(c, &alive) == Kind::Working)
                .filter_map(|c| c.sent.as_ref())
                .map(|s| s.session.clone())
                .filter(|s| !chats.edits.contains_key(s))
                .find(|s| Some(s.as_str()) != page)
        })?;
        let chat = chats.list.iter().find(|c| c.session == session)?;
        let (letter, hue) = chats.mark(&session);
        let name = if chat.title.is_empty() {
            chat.agent.clone()
        } else {
            chat.title.clone()
        };
        Some((name, letter, hue))
    }

    fn on_comments(&self, changed: OnChange, cx: &mut App) {
        cx.observe(&self.comments, move |_, cx| changed(cx))
            .detach();
    }

    fn take_answers(&self, cx: &mut App) -> Option<Vec<(u64, State, usize)>> {
        let bus = Bus::new()?;
        self.comments.update(cx, |c, _| c.take_answers(&bus))
    }

    fn notes<'a>(&self, cx: &'a App) -> &'a [Notes] {
        &self.notes.read(cx).all
    }

    fn followed(&self, id: u64, cx: &App) -> Option<u32> {
        self.notes.read(cx).lines.get(&id).copied()
    }

    fn change_notes(&self, change: &mut dyn FnMut(&mut Vec<Notes>), cx: &mut App) {
        self.notes.update(cx, |n, _| change(&mut n.all));
    }

    fn load_notes(&self, project: PathBuf, cx: &mut App) {
        self.notes.update(cx, |n, _| n.load(project));
    }

    fn save_notes(&self, cx: &App) {
        self.notes.read(cx).save();
    }

    fn follow(
        &self,
        root: Option<PathBuf>,
        shown: Option<Shown>,
        branch: Option<String>,
        cx: &mut App,
    ) {
        self.notes
            .update(cx, |n, cx| n.follow(root, shown, branch, cx));
    }

    fn on_followed(&self, done: OnChange, cx: &mut App) {
        cx.observe(&self.notes, move |_, cx| done(cx)).detach();
    }

    fn pick_chat(&self, ids: Vec<u64>, root: &Path, project: &Path, cx: &mut App) {
        self.card
            .update(cx, |card, cx| card.send(ids, root, project, cx));
    }

    fn pick_route(&self, branch: String, now: Option<String>, cx: &mut App) {
        self.card.update(cx, |card, cx| card.route(branch, now, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
