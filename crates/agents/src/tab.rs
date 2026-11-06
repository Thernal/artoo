//! A tab's agents implemented (#165 stage 4, `artoo_agents_api::tab`): its chats, its comments to them, the notes,
//! the card asking which chat and Claude Code being connected, each an entity its parts draw.

use crate::agents::Bus;
use crate::chats::Chats;
use crate::comments::AgentComments;
use crate::connect::Connect;
use crate::notes::AgentNotes;
use crate::pick::SendCard;
use artoo_agents_api::bus::Chat;
use artoo_agents_api::comment::{Comment, State};
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
        self.chats
            .update(cx, |c, cx| c.beat(root, here, &comments, cx))
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
