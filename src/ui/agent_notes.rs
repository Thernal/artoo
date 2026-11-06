//! An agent's notes on the lines of its changes (#151, `crate::notes`): drawn under their lines in
//! the agent's colour, apart from the reviewer's comments, with the summary over the diff, while
//! the diff shows the changes they are on — the working tree, or a head the agent named (its
//! branch's pages, a comparison or a request with that head). Answer opens a comment for the agent
//! on the note's line; the reviewer dismisses one, or all of them at once. The notes kept — taken,
//! dismissed —, which are on the diff and where they followed their lines are the agents' (`Agents`);
//! the tab hears them changed (`on_notes`).

use super::Workspace;
use super::nav::Page;
use super::rows::FileData;
use crate::bus::{Event, What};
use crate::comment::Place;
use crate::notes::{Notes, On, Shown};
use artoo_agents_api::view::{self, NoteEvent, NoteLook, OnNote};
use artoo_review_api::diff::{Diff, Header};
use gpui::{App, Context, WeakEntity, div, prelude::*};
use std::rc::Rc;

/// Show on a toast of an agent's notes: the changes `agent` left them on, opened under Branches
/// and shown (#151).
#[derive(Clone, PartialEq, Debug, gpui::Action)]
#[action(namespace = agents, no_json, no_register)]
pub(super) struct ShowNotes {
    pub(super) page: Page,
    pub(super) unstaged: Option<bool>,
    agent: String,
}

impl Workspace {
    /// An agent's notes on its changes (`What::Notes`) come in: kept, its earlier ones on the
    /// same changes replaced; the changes opened under Branches too when it asked, else a toast
    /// offers to show them.
    pub(super) fn take_agent_notes(&mut self, event: Event, cx: &mut Context<Self>) {
        let What::Notes {
            changes,
            summary,
            notes,
            open,
        } = event.what
        else {
            return;
        };
        let Some((page, unstaged)) = super::agent_events::changes_page(&changes) else {
            return;
        };
        let on = match *changes {
            What::Compare { head, head_id, .. } => On::Head {
                name: head,
                commit: head_id,
            },
            _ => On::WorkingTree,
        };
        let agent = event.agent;
        let count = notes.len();
        let notes = Notes {
            uid: event.uid,
            agent: agent.clone(),
            at: event.at,
            on,
            summary,
            notes,
        };
        self.agents.take_notes(notes, cx);
        self.map_agent_notes(cx);
        self.refresh_rows(cx);
        if open {
            return self.agent_changes(agent, page, unstaged, count, cx);
        }
        let what = super::agent_events::changes_said(&page);
        if let Some(item) = self
            .item_of(&page.route())
            .filter(|item| self.nav.opened(item).is_some())
        {
            let notes = super::plural(count, "note");
            self.call_reviewer(item, format!("{agent} left {notes} on it"));
        }
        let act = ShowNotes {
            page,
            unstaged,
            agent: agent.clone(),
        };
        let say = move |_| {
            let notes = super::plural(count, "note");
            format!("{agent} left {notes} on its changes: {what}")
        };
        self.agent_toast(act, "Show", 1, say, cx);
    }

    /// Show on a toast of notes: their changes open under Branches, on screen.
    pub(super) fn show_agent_notes(
        &mut self,
        page: Page,
        unstaged: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let page = page.route();
        let Some(item) = self.item_of(&page) else {
            return;
        };
        self.open_quietly(page, cx);
        self.save_nav();
        self.show_changes(&item, unstaged, cx);
    }

    /// The notes on the changes the diff shows (`notes::shown`), by the page's branch.
    pub(super) fn notes_shown<'a>(&self, cx: &'a App) -> Vec<&'a Notes> {
        let Some(diff) = self.shown_diff(cx) else {
            return Vec::new();
        };
        crate::notes::shown(self.agents.notes(cx), &shown(&diff), self.page_branch(cx))
    }

    /// The notes shown on lines of file `path`, built as `data`: (id, line on the new side).
    pub(super) fn notes_in(&self, path: &str, data: &FileData, cx: &App) -> Vec<(u64, u32)> {
        let side = data.side(false);
        self.notes_shown(cx)
            .into_iter()
            .flat_map(|n| n.notes.iter())
            .filter(|n| n.path == path)
            .filter_map(|n| {
                match crate::comment::find_line(n.line, &n.code, side.count(), |l| side.line(l).0) {
                    Place::Line(line) => Some((n.id, line)),
                    Place::Outdated => self.agents.followed(n.id, cx).map(|line| (n.id, line)),
                }
            })
            .collect()
    }

    /// Follows the line of every note on a head the diff has moved on from, from the commit it was
    /// written on to the one shown, in the background (#46, `Agents::follow`).
    pub(super) fn map_agent_notes(&mut self, cx: &mut Context<Self>) {
        let (root, shown, branch) = (
            self.root.clone(),
            self.shown_diff(cx).as_deref().map(shown),
            self.page_branch(cx),
        );
        self.agents.follow(root, shown, branch, cx);
    }

    /// Answer on a note: a comment for the agent on its line, as comments for it are written; it
    /// carries the note, so the agent reads what it answers.
    pub(super) fn answer_note(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some((path, body)) = crate::notes::note(self.agents.notes(cx), id)
            .map(|(_, n)| (n.path.clone(), n.body.clone()))
        else {
            return;
        };
        // In one scroll of all the files, its file becomes the one the keyboard works in.
        if let Some(file) = self
            .shown_diff(cx)
            .as_ref()
            .and_then(|d| d.files.iter().position(|f| f.path() == path))
            && self.rv().stacked(cx)
        {
            self.activate_file(file, cx);
        }
        let Some(line) = self
            .diff_data(cx)
            .and_then(|data| {
                self.notes_in(&path, &data, cx)
                    .into_iter()
                    .find(|n| n.0 == id)
            })
            .map(|(_, line)| line)
        else {
            return;
        };
        self.start_comment(false, line, cx);
        if let Some(compose) = &mut self.compose {
            compose.target = super::composer::Target::Agent;
            compose.answers = Some(body);
        }
        cx.notify();
    }

    /// Answer pressed on the first note on the file shown (the snapshot checks).
    #[cfg(feature = "snapshot")]
    pub(super) fn snapshot_answer_note(&mut self, cx: &mut Context<Self>) {
        let path = self
            .shown_diff(cx)
            .as_ref()
            .and_then(|d| d.files.get(self.diff_file(cx)))
            .map(|f| f.path().to_owned());
        let first = match (&self.diff_data(cx), path) {
            (Some(data), Some(path)) => self.notes_in(&path, data, cx).first().map(|&(id, _)| id),
            _ => None,
        };
        if let Some(id) = first {
            self.answer_note(id, cx);
        }
    }

    /// What answers a note's clicks: Answer the tab's composer, Dismiss the agents'.
    fn note_on(this: WeakEntity<Self>) -> OnNote {
        Rc::new(move |event, cx| {
            this.update(cx, |this, cx| match event {
                NoteEvent::Answer(id) => this.answer_note(id, cx),
                NoteEvent::Dismiss(id) => this.agents.dismiss_note(id, cx),
                NoteEvent::DismissAll(uid) => this.agents.dismiss_notes(&uid, cx),
            })
            .ok();
        })
    }

    /// Note `id` under its line (`artoo_agents_api::view::note_card`).
    pub(super) fn render_agent_note(
        &self,
        id: u64,
        indent: f32,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> gpui::AnyElement {
        let Some((notes, note)) = crate::notes::note(self.agents.notes(cx), id) else {
            return div().into_any_element();
        };
        let look = NoteLook {
            mark: self.agents.agent_mark(&notes.agent, cx),
            rich: self.settings.rich_comments,
            tight: self.tight(cx),
            selecting: self.selecting(this.clone()),
        };
        view::note_card(notes, note, indent, look, Self::note_on(this), cx)
    }
}

/// What `diff` is of, as the agents' notes ask it.
fn shown(diff: &Diff) -> Shown {
    match (&diff.header, diff.header.head()) {
        (Header::WorkingTree { .. }, _) | (_, None) => Shown::WorkingTree,
        (header, Some(head)) => Shown::Head {
            commit: head.to_string(),
            name: match header {
                Header::Compare { head, .. } => Some(head.0.clone()),
                _ => None,
            },
        },
    }
}

/// What the agents' notes' summary asks of the tab (`artoo_agents_api::view::NotesHost`).
pub(super) struct NotesHost(pub WeakEntity<Workspace>);

impl view::NotesHost for NotesHost {
    fn branch(&self, cx: &App) -> Option<String> {
        self.0.upgrade()?.read(cx).page_branch(cx)
    }

    fn rich(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|tab| tab.read(cx).settings.rich_comments)
    }

    fn note(&self, event: NoteEvent, cx: &mut App) {
        Workspace::note_on(self.0.clone())(event, cx)
    }
}
