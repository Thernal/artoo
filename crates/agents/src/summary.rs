//! Over the diff, the summary of each agent's notes on the changes shown (#151): a piece of the item page
//! (`Slot::OverDiff`), drawn as a view of its own so it reads the tab's answers through `NotesHost` — the page's
//! branch, an agent's mark, how comments are drawn — and the diff shown through the review's handle.

use crate::notes::{AgentNotes, shown_of};
use crate::view::{self, NoteEvent, OnNote};
use artoo_review_api::Review;
use artoo_shell_api::{Frame, Piece, Slot};
use artoo_ui_kit::gap;
use gpui::{AnyView, App, Context, Entity, Render, Rgba, Window, div, prelude::*};
use std::rc::Rc;

/// What the notes' summary asks of the tab (`Rc<dyn NotesHost>` in its services).
pub trait NotesHost {
    /// The branch the page is of: a request's source, the branch open in Branches, else the one checked out.
    fn branch(&self, cx: &App) -> Option<String>;

    /// Agent `agent`'s mark: its latest chat's letter and colour.
    fn mark(&self, agent: &str, cx: &App) -> (char, Rgba);

    /// Comments drawn as markdown.
    fn rich(&self, cx: &App) -> bool;

    /// A click on a note or a summary: answered, dismissed.
    fn note(&self, event: NoteEvent, cx: &mut App);
}

/// The summary in one tab.
struct Summary {
    notes: Entity<AgentNotes>,
    frame: Rc<dyn Frame>,
}

/// The summary as a piece over the diff.
pub fn piece(notes: Entity<AgentNotes>, frame: Rc<dyn Frame>, cx: &mut App) -> Piece {
    let view: AnyView = cx
        .new(|cx| {
            cx.observe(&notes, |_, _, cx| cx.notify()).detach();
            Summary { notes, frame }
        })
        .into();
    Piece {
        id: "agents.summary",
        slot: Slot::OverDiff,
        draw: Rc::new(move |_, _, _| Some(view.clone().into_any_element())),
    }
}

impl Render for Summary {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let services = self.frame.services();
        let (Some(host), Some(review), Some(selecting)) = (
            services.get::<Rc<dyn NotesHost>>(),
            services.get::<Rc<dyn Review>>(),
            self.frame.selecting(cx),
        ) else {
            return div().into_any_element();
        };
        let Some(diff) = review.shown(cx) else {
            return div().into_any_element();
        };
        let notes = self.notes.read(cx);
        let shown: Vec<_> =
            artoo_agents_api::notes::shown(&notes.all, &shown_of(&diff), host.branch(cx))
                .into_iter()
                .map(|n| (n, host.mark(&n.agent, cx)))
                .collect();
        let rich = host.rich(cx);
        let on: OnNote = Rc::new(move |event, cx| host.note(event, cx));
        view::notes_summary(&shown, rich, selecting, gap(), on, cx)
            .unwrap_or_else(|| div().into_any_element())
    }
}
