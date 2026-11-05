//! How the agents are drawn where the tab draws them (#165 stage 4, a render contract as the frame's): a chat's badge,
//! an agent's note under its line, its form under its question, an answer in the Inbox — their params, what a click
//! asks the tab, the renderer and a facade each; the drawing is the agents', whose `install` provides the renderer.
//! With none installed a facade draws nothing. What the Agents window and the notes' summary ask of the tab are its
//! hosts (`AgentsHost`, `NotesHost`).

use crate::comment::{Answered, Comment, Form};
use crate::notes::{Note, Notes};
use artoo_ui_kit::markdown::Selecting;
use gpui::{AnyElement, App, Div, Global, Rgba, Stateful, div, prelude::*};
use std::rc::Rc;

/// The Agents window's id: the layout keeps it open by it.
pub const WINDOW: &str = "agents";

/// Settings' section of the agents, by its id: connecting Claude Code and the others.
pub const SETTINGS: &str = "agents";

/// What the Agents window asks of the tab (`Rc<dyn AgentsHost>` in its services).
pub trait AgentsHost {
    /// The branch the page is of, where a note written now goes (#77).
    fn branch(&self, cx: &App) -> Option<String>;

    /// Comments drawn as markdown: a row's text on one line without it.
    fn rich(&self, cx: &App) -> bool;

    /// A click in the window that changes a comment or opens what is the tab's.
    fn act(&self, act: Act, cx: &mut App);
}

/// What a click in the Agents window asks of the tab.
#[derive(Clone, Debug)]
pub enum Act {
    /// Comment `id` shown on its line.
    Show(u64),
    /// These sent, to the chat their route takes.
    Send(Vec<u64>),
    /// The card that sends every draft to the chat picked in it.
    PickChat,
    /// These sent to chat `session` ("" a new chat, opened in a terminal), picked on the card.
    SendTo {
        ids: Vec<u64>,
        session: String,
        title: String,
    },
    /// The card that picks the chat branch `.0`'s notes go to ("" for any).
    Route(String),
    /// A draft written again.
    Edit(u64),
    /// A draft gone, no question asked; the toast's Undo brings it back.
    Delete(u64),
    /// A note its gone chat never took, to the chat its route takes now (#77).
    Resend(u64),
    /// Sent and not taken yet: back to the drafts.
    Withdraw(u64),
    /// Archived, or back from the archive.
    Archive(u64, bool),
    /// Every resolved one archived.
    ArchiveResolved,
    Resolve(u64),
    /// A chat named (the developer, 2026-10-23: chats mixed up).
    NameChat {
        session: String,
        agent: String,
    },
}

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

/// A click on a note or a summary.
#[derive(Clone)]
pub enum NoteEvent {
    /// Answer: a comment for the agent on note `id`'s line.
    Answer(u64),
    /// Note `id` dismissed.
    Dismiss(u64),
    /// Every note of the agent's notes `uid` dismissed.
    DismissAll(String),
}

/// What answers a note's clicks.
pub type OnNote = Rc<dyn Fn(NoteEvent, &mut App)>;

/// What a note is drawn with that the tab knows.
pub struct NoteLook {
    /// The agent's mark: its chat's letter and colour.
    pub mark: (char, Rgba),
    /// Its markdown drawn rich.
    pub rich: bool,
    /// A narrow diff: Answer is an icon alone.
    pub tight: bool,
    pub selecting: Selecting,
}

/// A click on an agent's form.
#[derive(Clone, Debug)]
pub enum FormEvent {
    /// Choice `.1` of comment `.0`'s form picked, or unpicked where many are; `None`, your call.
    Pick(u64, Option<usize>),
    /// An order's choice at place `.1` moved up (`.2`) or down.
    Move(u64, usize, bool),
    /// One's own value written.
    Own(u64),
    /// A note added to the answer.
    Note(u64),
    /// Comment `.0` sent alone.
    SendOnly(u64),
    /// Every draft sent.
    SendAll,
}

/// What answers a form's clicks.
pub type OnForm = Rc<dyn Fn(FormEvent, &mut App)>;

/// What a click on an agent's answer asks the tab.
pub enum AnswerEvent {
    /// Note `0`'s row clicked: selected, or opened where the tab is narrow (#80).
    Pick(u64),
    /// Note `0` opened at its line.
    Open(u64),
    /// For later; its "done" resolved.
    Done,
}

/// How the tab answers clicks on the agents' answers.
pub type OnAnswer = Rc<dyn Fn(AnswerEvent, &mut App)>;

/// Who an answer is from: the chat's agent ("Claude") and the chat's title.
pub struct From {
    pub agent: String,
    pub chat: String,
}

/// How an answer's row looks.
pub struct AnswerLook {
    /// Not looked at since the agent last wrote: bold, its dot purple.
    pub fresh: bool,
    /// Selected.
    pub on: bool,
    /// The list has 260 px (#79): when, and the start of what it wrote.
    pub wide: bool,
}

/// The agents' pieces the tab draws, the agents' drawing.
pub trait Renderer {
    fn chat_badge(&self, letter: char, hue: Rgba, gone: bool) -> Div;

    fn note_card(
        &self,
        notes: &Notes,
        note: &Note,
        indent: f32,
        look: NoteLook,
        on: OnNote,
        cx: &App,
    ) -> AnyElement;

    fn form(
        &self,
        c: &Comment,
        drafts: &[u64],
        lines: Option<&dyn Fn(u32) -> Option<String>>,
        on: OnForm,
    ) -> Option<AnyElement>;

    fn record(&self, form: &Form, answered: Option<&Answered>) -> AnyElement;

    fn answer_row(&self, c: &Comment, from: From, look: AnswerLook, on: &OnAnswer)
    -> Stateful<Div>;

    fn answer_preview(
        &self,
        c: &Comment,
        from: From,
        thread: AnyElement,
        on: &OnAnswer,
    ) -> AnyElement;
}

struct Installed(Rc<dyn Renderer>);

impl Global for Installed {}

/// `renderer` draws the agents' pieces from now on, in every window (the agents' `install`).
pub fn provide(cx: &mut App, renderer: Rc<dyn Renderer>) {
    cx.set_global(Installed(renderer));
}

fn renderer(cx: &App) -> Option<Rc<dyn Renderer>> {
    cx.try_global::<Installed>().map(|r| r.0.clone())
}

/// A chat's mark: its letter on its hue, hollow when it is gone.
pub fn chat_badge(letter: char, hue: Rgba, gone: bool, cx: &App) -> Div {
    match renderer(cx) {
        Some(r) => r.chat_badge(letter, hue, gone),
        None => div(),
    }
}

/// Note `note` of `notes` under its line, `indent` from the left.
pub fn note_card(
    notes: &Notes,
    note: &Note,
    indent: f32,
    look: NoteLook,
    on: OnNote,
    cx: &App,
) -> AnyElement {
    match renderer(cx) {
        Some(r) => r.note_card(notes, note, indent, look, on, cx),
        None => div().into_any_element(),
    }
}

/// Under the agent's question: its form, answered with clicks; the foot sends (#10 C). `drafts` the comments the
/// next sending takes; `lines` the file's own lines, when it is the one open.
pub fn form(
    c: &Comment,
    drafts: &[u64],
    lines: Option<&dyn Fn(u32) -> Option<String>>,
    on: OnForm,
    cx: &App,
) -> Option<AnyElement> {
    renderer(cx)?.form(c, drafts, lines, on)
}

/// Under an earlier question of the agent's: its form as it was, what was picked marked.
pub fn record(form: &Form, answered: Option<&Answered>, cx: &App) -> AnyElement {
    match renderer(cx) {
        Some(r) => r.record(form, answered),
        None => div().into_any_element(),
    }
}

/// The answer to note `c` (#116) as the Inbox lists it.
pub fn answer_row(
    c: &Comment,
    from: From,
    look: AnswerLook,
    on: &OnAnswer,
    cx: &App,
) -> Stateful<Div> {
    match renderer(cx) {
        Some(r) => r.answer_row(c, from, look, on),
        None => div().id(("agents-answer", c.id)),
    }
}

/// The answer to note `c` as the Inbox previews it, under `thread`.
pub fn answer_preview(
    c: &Comment,
    from: From,
    thread: AnyElement,
    on: &OnAnswer,
    cx: &App,
) -> AnyElement {
    match renderer(cx) {
        Some(r) => r.answer_preview(c, from, thread, on),
        None => div().into_any_element(),
    }
}
