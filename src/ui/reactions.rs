//! Reactions on a request's notes (Design #9 B), the tab's side: the requests draw the pills, the hover bar and
//! ☺+, and keep ☺+ (`artoo_requests::reactions`); the tab toggles a reaction with its host, opens and closes ☺+
//! drawing the rows again, gives it the keyboard's keys, and ⇧E opens it on the selected discussion's newest
//! note.

use super::Workspace;
use artoo_requests::reactions::GridKey;
use gpui::Context;

impl Workspace {
    /// Reads the reactions on every note of the open request (`Requests::load_awards`).
    pub(super) fn load_awards(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests.update(cx, |rq, cx| rq.load_awards(host, cx));
    }

    /// Adds your reaction `name` to note `note`, or takes it back (`Requests::toggle_reaction`).
    pub(super) fn toggle_reaction(&mut self, note: u64, name: &str, cx: &mut Context<Self>) {
        // A pill added pops in (#56, 11).
        self.mark_motion(&format!("award-{note}-{name}"), cx);
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.toggle_reaction(note, name, host, cx));
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Reply with the note's author already mentioned (#9 B, #51).
    pub(super) fn reply_mentioning(&mut self, thread: usize, login: &str, cx: &mut Context<Self>) {
        self.start_reply(thread, cx);
        if !login.is_empty() {
            self.insert_into_compose(&format!("@{login} "));
        }
        cx.notify();
    }

    /// ⇧E: ☺+ on the newest note of the selected discussion, its search taking the keyboard.
    pub(super) fn react_focused(&mut self, cx: &mut Context<Self>) {
        let Some(ix) = self
            .acting_thread(cx)
            .filter(|_| self.request_target(cx).is_some())
        else {
            return;
        };
        let Some(note) = self
            .requests
            .read(cx)
            .threads
            .get(ix)
            .and_then(|t| t.notes.last())
        else {
            return;
        };
        self.thread_at = Some(ix);
        self.open_reaction_grid(note.id, cx);
    }

    pub(super) fn open_reaction_grid(&mut self, note: u64, cx: &mut Context<Self>) {
        self.requests
            .update(cx, |rq, _| rq.open_reaction_grid(note));
        self.refresh_rows(cx);
        cx.notify();
    }

    pub(super) fn close_reaction_grid(&mut self, cx: &mut Context<Self>) {
        self.requests.update(cx, |rq, _| rq.close_reaction_grid());
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Keys while ☺+ is open (`Requests::reaction_key`): ↵ reacts with the one picked, esc closes.
    pub(super) fn reaction_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        let github = self.on_github();
        let Some(note) = self.requests.read(cx).grid_note() else {
            return;
        };
        match self
            .requests
            .update(cx, |rq, cx| rq.reaction_key(event, github, cx))
        {
            Some(GridKey::React(name)) => {
                if let Some(name) = name {
                    self.toggle_reaction(note, &name, cx);
                }
                self.close_reaction_grid(cx);
            }
            Some(GridKey::Close) => self.close_reaction_grid(cx),
            None => cx.notify(),
        }
    }
}
