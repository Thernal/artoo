//! The composer (#9, #51, #129, #168): a + in the gutter opens it under the line — on a page, a file or a folder
//! in Code too —, for the agent, a merge request, a reply or an edit; its keys routed, the completion's choices,
//! who it goes to and what its foot's buttons do. The comment itself, its editing and its drawing, the foot's
//! too, are the review's (`artoo_review::composer`). What it writes
//! goes to its owner: the agents' comments (`agent_comments.rs`), a
//! request (`replies.rs`). Comments for an agent are kept on this Mac (`crate::review`), never in the reviewed
//! repository. Designed in `../Design/mockups/line-comments/a-agent-comments.html`.

use super::discussion::{self, Kind};
use super::input;
use super::markdown;
use super::px;
use super::typing::{self, Input};
use super::waiting::Op;
use super::{Selection, Workspace, format};
use crate::markdown::Helper;
use crate::review::{Comment, State};
use crate::textedit::TextEdit;
pub(super) use artoo_review::composer::{
    Change, Compose, ComposeAct, ComposeActs, FootEvent, FootLook, Look, Room, Short, Target,
    Typed, room_for,
};
use gpui::{App, Context, KeyDownEvent, WeakEntity, div, prelude::*};

/// The quiet Edit on one's own note: a pencil, alone in a narrow diff (#69).
pub(super) fn edit_button(
    id: impl Into<gpui::ElementId>,
    key: Option<&'static str>,
    bare: bool,
    change: Change,
    this: WeakEntity<Workspace>,
) -> gpui::AnyElement {
    discussion::icon_button(id, "pencil", "Edit", key, None, Kind::Quiet, bare)
        .h(px(22.))
        .px(px(6.))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            this.update(cx, |this, cx| this.start_edit(change, cx)).ok();
        })
        .into_any_element()
}

impl Workspace {
    /// The lines selected in the file read in Code, first and last (1-based); a selection that
    /// ends at the start of a line does not take that line (#104).
    pub(super) fn file_selected_lines(&self, cx: &App) -> Option<(u32, u32)> {
        let view = self.review.read(cx).file_view.as_ref()?;
        let sel = self
            .note_sel
            .as_ref()
            .filter(|s| s.note.as_ref() == format!("file-lines-{}", view.path))?;
        if sel.anchor == sel.head {
            return None;
        }
        let ((first, _), (last, at)) = sel.ends();
        let last = if at == 0 && last > first {
            last - 1
        } else {
            last
        };
        Some((first as u32 + 1, last as u32 + 1))
    }

    pub(super) fn start_comment(&mut self, old: bool, line: u32, cx: &mut Context<Self>) {
        self.start_comment_on(old, None, line, cx);
    }

    /// Where a comment is written, for the reader of the export: a commit or a version range.
    fn comment_at(&self, cx: &App) -> String {
        match self.history.read(cx).selection {
            Selection::Commit(ix) => self
                .history
                .read(cx)
                .commits
                .get(ix)
                .map(|c| format::short(c.id))
                .unwrap_or_default(),
            Selection::Commits { newest, oldest } => {
                match (
                    self.history.read(cx).commits.get(oldest),
                    self.history.read(cx).commits.get(newest),
                ) {
                    (Some(a), Some(b)) => {
                        format!("{}..{}", format::short(a.id), format::short(b.id))
                    }
                    _ => String::new(),
                }
            }
            Selection::Versions { from, to } => {
                match (
                    self.history.read(cx).versions.get(from),
                    self.history.read(cx).versions.get(to),
                ) {
                    (Some(a), Some(b)) => format!("v{}→v{}", a.number, b.number),
                    _ => String::new(),
                }
            }
            Selection::WorkingTree => "working tree".to_owned(),
            Selection::None => String::new(),
        }
    }

    /// Who a new comment is for: a run of comments to one merge request does not have to pick it
    /// every time.
    fn comment_target(&self, cx: &App) -> Target {
        match self.request_target(cx) {
            Some(iid) if self.last_request_target == Some(iid) => Target::Request(iid),
            _ => Target::Agent,
        }
    }

    /// A comment on a block of the document file `ix` read as a page: on the source line `line`
    /// the block starts on, written under the block (#11).
    pub(super) fn start_page_comment(
        &mut self,
        ix: usize,
        old: bool,
        line: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .and_then(|d| d.files.get(ix))
        else {
            return;
        };
        let text = if old { &file.old_text } else { &file.new_text };
        let code = text
            .as_deref()
            .and_then(|t| t.lines().nth(line as usize - 1))
            .unwrap_or_default()
            .to_owned();
        let path = file.path().to_owned();
        self.compose = Some(Compose {
            path,
            old,
            first: None,
            line,
            code,
            edit: TextEdit::default(),
            at: self.comment_at(cx),
            target: self.comment_target(cx),
            error: None,
            preview: false,
            pick: 0,
            closed: None,
            page: Some(ix),
            file: None,
            answers: None,
        });
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        self.review.update(cx, |rv, _| rv.remeasure_pages());
        cx.notify();
    }

    /// A note on line `line` of the file read in Code: for the agent, kept on the path, the line
    /// and its text, written under the line (#75).
    pub(super) fn start_file_comment(&mut self, line: u32, cx: &mut Context<Self>) {
        self.start_file_comment_on(None, line, cx)
    }

    /// A note on lines `first`…`line` of the file read in Code (#104), under the last.
    pub(super) fn start_file_comment_on(
        &mut self,
        first: Option<u32>,
        line: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = &self.review.read(cx).file_view else {
            return;
        };
        let Some(code) = view.line(line) else {
            return;
        };
        self.compose = Some(Compose {
            path: view.path.clone(),
            old: false,
            first: first.filter(|&f| f < line),
            line,
            code: code.to_owned(),
            edit: TextEdit::default(),
            at: view.at().to_owned(),
            target: Target::Agent,
            error: None,
            preview: false,
            pick: 0,
            closed: None,
            page: None,
            file: Some(view.rev()),
            answers: None,
        });
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        cx.notify();
    }

    /// How much of its foot the field being written has room for (`room_for`). Its own width
    /// decides, wherever it is — the diff, a file in Code, a folder's page —, or the narrow diff's.
    pub(super) fn compose_room(&self, cx: &App) -> Room {
        room_for(self.widths.width("composer"), self.review.read(cx).tight)
    }

    /// Under L: the secondary buttons keep only their icon, named in the tooltip, the keycaps go,
    /// the primary keeps its label (Design, 2026-10-21, C · icons below a width).
    pub(super) fn compose_narrow(&self, cx: &App) -> bool {
        self.compose_room(cx) != Room::L
    }

    /// A note for the agent on folder `path` of Code (#97): kept on the path with a slash and no
    /// line, written on the folder's page.
    pub(super) fn start_folder_comment(&mut self, path: &str, cx: &mut Context<Self>) {
        let rev = self.code().and_then(|c| c.revision(cx));
        let at = match &rev {
            Some((_, id)) => super::format::short(*id),
            None => "working tree".to_owned(),
        };
        self.compose = Some(Compose {
            path: format!("{path}/"),
            old: false,
            first: None,
            line: 0,
            code: String::new(),
            edit: TextEdit::default(),
            at,
            target: Target::Agent,
            error: None,
            preview: false,
            pick: 0,
            closed: None,
            page: None,
            file: Some(rev.map(|(_, id)| id)),
            answers: None,
        });
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        cx.notify();
    }

    /// The folder a note for the agent is being written on (#97).
    pub(super) fn folder_compose(&self) -> Option<&str> {
        let c = self.compose.as_ref().filter(|c| c.file.is_some())?;
        c.path.strip_suffix('/')
    }

    /// The composer written on Code's file: its path and line.
    pub(super) fn file_compose(&self) -> Option<(&str, u32)> {
        let c = self.compose.as_ref().filter(|c| c.file.is_some())?;
        Some((&c.path, c.line))
    }

    /// The composer written on a page: its file, side and line.
    pub(super) fn page_compose(&self) -> Option<(usize, bool, u32)> {
        let c = self.compose.as_ref()?;
        Some((c.page?, c.old, c.line))
    }

    /// A comment on line `line`, or on `first`..=`line`.
    pub(super) fn start_comment_on(
        &mut self,
        old: bool,
        first: Option<u32>,
        line: u32,
        cx: &mut Context<Self>,
    ) {
        let (Some(data), Some(path)) = (
            &self.review.read(cx).data,
            self.review.read(cx).current_path(),
        ) else {
            return;
        };
        let code = data.side(old).line(line).0.to_owned();
        let (at, target) = (self.comment_at(cx), self.comment_target(cx));
        self.compose = Some(Compose {
            path,
            old,
            first: first.filter(|&f| f < line),
            line,
            code,
            edit: TextEdit::default(),
            at,
            target,
            error: None,
            preview: false,
            pick: 0,
            closed: None,
            page: None,
            file: None,
            answers: None,
        });
        self.take_keyboard(Input::Compose, cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Edit: the composer in place of one's own comment, its text in it.
    pub(super) fn start_edit(&mut self, change: Change, cx: &mut Context<Self>) {
        let (path, old, line, text) = match change {
            Change::Agent(id) => {
                let Some(c) = self.comments.iter().find(|c| c.id == id) else {
                    return;
                };
                if !self.toggled_agent.contains(&id) && c.state == State::Resolved {
                    self.toggled_agent.insert(id);
                }
                (c.path.clone(), c.old, c.line, c.body.clone())
            }
            Change::Note { thread, note } => {
                let Some(t) = self.requests.read(cx).threads.get(thread) else {
                    return;
                };
                let Some(n) = t
                    .notes
                    .iter()
                    .find(|n| n.id == note && !super::waiting::is_temp(n.id))
                else {
                    return;
                };
                self.thread_at = Some(thread);
                if !self.mr_open(thread, cx) {
                    self.toggled_mr.insert(thread);
                }
                let old = t.new_line.is_none();
                let line = t.new_line.or(t.old_line).unwrap_or(0);
                (
                    t.path.clone().unwrap_or_default(),
                    old,
                    line,
                    n.body.clone(),
                )
            }
        };
        let mut edit = TextEdit::new(text);
        edit.move_to(crate::textedit::Move::End, false);
        self.compose = Some(Compose {
            path,
            old,
            first: None,
            line,
            code: String::new(),
            edit,
            at: String::new(),
            target: Target::Edit(change),
            error: None,
            preview: false,
            pick: 0,
            closed: None,
            page: None,
            file: None,
            answers: None,
        });
        self.take_keyboard(Input::Compose, cx);
        self.reset_field_edit();
        self.refresh_rows(cx);
        cx.notify();
    }

    /// `e`: edit one's own last note in the discussion with the keyboard.
    pub(super) fn edit_focused(&mut self, cx: &mut Context<Self>) {
        if self.compose.is_some() {
            return;
        }
        let Some(thread) = self.acting_thread(cx) else {
            return;
        };
        match self.my_note_in(thread, cx) {
            Some(note) => self.start_edit(Change::Note { thread, note }, cx),
            None => {
                self.error = Some("Nothing of yours to edit in this discussion".into());
                cx.notify();
            }
        }
    }

    /// Saves an edit: an agent's comment here, a note on GitLab or GitHub.
    fn save_edit(&mut self, change: Change, cx: &mut Context<Self>) {
        let Some(compose) = self.compose.as_ref() else {
            return;
        };
        let body = compose.edit.text().trim().to_owned();
        if body.is_empty() {
            return;
        }
        match change {
            Change::Agent(id) => {
                if let Some(c) = self.comments.iter_mut().find(|c| c.id == id) {
                    c.body = body;
                }
                self.compose = None;
                self.save_comments();
                self.copied = false;
                self.refresh_rows(cx);
                cx.notify();
            }
            Change::Note { thread, note } => {
                let (Some(t), Some(iid)) = (
                    self.requests.read(cx).threads.get(thread),
                    self.requests.read(cx).current,
                ) else {
                    return;
                };
                let id = t.id.clone();
                let op = Op::Edit {
                    note,
                    body: body.clone(),
                };
                self.submit_compose(iid, vec![op], cx, move |host| {
                    host.edit_note(iid, &id, note, &body)
                });
            }
        }
    }

    /// The editor of an edit, when `change` is being edited.
    pub(super) fn edit_box(
        &self,
        change: Change,
        this: WeakEntity<Self>,
        cx: &gpui::App,
    ) -> Option<gpui::Div> {
        let compose = self
            .compose
            .as_ref()
            .filter(|c| c.target == Target::Edit(change))?;
        let on = this.clone();
        let tail = compose.edit_foot(
            self.compose_narrow(cx),
            std::rc::Rc::new(move |event, cx| {
                on.update(cx, |this, cx| this.foot_event(event, cx)).ok();
            }),
        );
        Some(self.open_input(
            compose,
            compose.source().color(),
            "The comment…",
            None,
            &[
                Helper::Bold,
                Helper::Italic,
                Helper::Code,
                Helper::Link,
                Helper::Quote,
                Helper::Bullet,
                Helper::CodeBlock,
            ],
            tail,
            this,
            cx,
        ))
    }

    /// The merge request whose diff is open, when a comment can be posted to it.
    pub(super) fn request_target(&self, cx: &App) -> Option<u64> {
        match self.review.read(cx).diff.as_ref()?.header {
            super::Header::Compare {
                request: Some(iid),
                since_merge_base: true,
                ..
            } if self.host.as_ref().is_some_and(|h| h.available()) => Some(iid),
            _ => None,
        }
    }

    /// The request the open composer can post to: none for a note on a file or folder in Code,
    /// which is the agent's only, whatever request's diff is loaded behind it (#130).
    fn compose_request(&self, cx: &App) -> Option<u64> {
        self.request_target(cx)
            .filter(|_| self.compose.as_ref().is_some_and(|c| c.file.is_none()))
    }

    fn toggle_target(&mut self, cx: &mut Context<Self>) {
        let request = self.compose_request(cx);
        if let Some(compose) = self.compose.as_mut() {
            compose.target = match (compose.target, request) {
                // ⇥ in a discussion's reply: a note for the agent instead, and back (#50 G).
                (Target::Reply(ix), _) => Target::ThreadNote(ix),
                (Target::ThreadNote(ix), _) => Target::Reply(ix),
                (Target::Answer(id), _) => Target::Answer(id),
                (Target::AnswerValue(id), _) => Target::AnswerValue(id),
                (Target::Edit(change), _) => Target::Edit(change),
                (Target::InboxReply, _) => Target::InboxReply,
                (Target::Agent, Some(iid)) => Target::Request(iid),
                _ => Target::Agent,
            };
            compose.error = None;
            cx.notify();
        }
    }

    pub(super) fn cancel_comment(&mut self, cx: &mut Context<Self>) {
        if self.compose.take().is_some() {
            self.refresh_rows(cx);
            self.review.update(cx, |rv, _| rv.remeasure_pages());
            cx.notify();
        }
    }

    pub(super) fn add_comment(&mut self, cx: &mut Context<Self>) {
        match self.compose.as_ref().map(|c| c.target) {
            Some(Target::Request(iid)) => return self.post_comment(iid, false, cx),
            Some(Target::Reply(ix)) => return self.post_reply(ix, cx),
            Some(Target::Edit(change)) => return self.save_edit(change, cx),
            Some(Target::Answer(id)) => return self.save_answer_note(id, cx),
            Some(Target::AnswerValue(id)) => return self.save_answer_value(id, cx),
            Some(Target::ThreadNote(ix)) => return self.add_thread_note(ix, cx),
            Some(Target::InboxReply) => return self.post_inbox_reply(cx),
            _ => {}
        }
        let Some(compose) = self.compose.take() else {
            return;
        };
        if compose.edit.text().trim().is_empty() {
            self.compose = Some(compose);
            return;
        }
        self.last_request_target = None;
        let id = self.comments.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let above = match (
            compose.first,
            &self.review.read(cx).data,
            &self.review.read(cx).file_view,
        ) {
            // On a file read in Code: its own lines, whatever diff is loaded (#104).
            (Some(first), _, Some(view)) if compose.file.is_some() => (first..compose.line)
                .filter_map(|n| view.line(n).map(str::to_owned))
                .collect(),
            (Some(first), Some(data), _) if compose.file.is_none() => (first..compose.line)
                .map(|n| data.side(compose.old).line(n).0.to_owned())
                .collect(),
            _ => Vec::new(),
        };
        self.comments.push(Comment {
            id,
            path: compose.path,
            old: compose.old,
            line: compose.line,
            code: compose.code,
            above,
            body: compose.edit.text().trim().to_owned(),
            at: compose.at,
            commit: match compose.file {
                Some(rev) => rev.map(|id| id.to_string()),
                None => self.review.read(cx).shown_head().map(|id| id.to_string()),
            },
            branch: self.page_branch(cx),
            answers: compose.answers,
            ..Default::default()
        });
        self.save_comments();
        self.copied = false;
        self.refresh_rows(cx);
        self.review.update(cx, |rv, _| rv.remeasure_pages());
        cx.notify();
    }

    /// Typing into the composer: a real text field (caret, selection, ⌥ words, ⌘ lines, undo),
    /// return carries a list on, ⇧↵ a plain new line; ⌘B ⌘I ⌘⇧X ⌘⇧E ⌘⇧K for markdown, ⌘⇧P to
    /// preview it, Tab for who sees it, ⌘↵ to send, ⇧⌘↵ to add it to the review, esc to cancel —
    /// or, written in, to leave it.
    pub(super) fn compose_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = &event.keystroke;
        let m = key.modifiers;
        if self.compose.is_none() {
            return;
        }
        if key.key == "enter" && m.secondary() {
            let target = self.compose.as_ref().map(|c| c.target);
            if m.shift
                && let Some(Target::Request(iid)) = target
            {
                return self.post_comment(iid, true, cx);
            }
            // An agent comment: ⇧⌘↵ adds it and sends all drafts, ⌥⌘↵ sends this one only.
            if target == Some(Target::Agent) && (m.shift || m.alt) {
                return self.add_and_send(m.alt, cx);
            }
            return self.add_comment(cx);
        }
        if let Some((at, choices)) = self.compose_completion(cx)
            && let Some(changed) = self
                .compose
                .as_mut()
                .and_then(|c| c.completion_key(key, at, &choices))
        {
            if changed {
                self.review.read(cx).diff_list.remeasure();
            }
            return cx.notify();
        }
        if key.key == "tab" {
            return self.toggle_target(cx);
        }
        if self.vertical_key(key, cx) {
            return;
        }
        match Compose::shortcut(key) {
            Some(Short::Helper(helper)) => return self.apply_helper(helper, cx),
            Some(Short::Preview) => return self.toggle_preview(cx),
            None => {}
        }
        let Some(compose) = self.compose.as_mut() else {
            return;
        };
        match compose.edit_key(key, cx) {
            Typed::Leave => return self.leave_input(cx),
            Typed::Cancel => return self.cancel_comment(cx),
            Typed::Ignored => return,
            Typed::Changed => {}
        }
        // The box grows with its text.
        self.review.read(cx).diff_list.remeasure();
        cx.notify();
    }

    /// Puts `text` at the composer's caret (a reply that mentions the author).
    pub(super) fn insert_into_compose(&mut self, text: &str) {
        if let Some(c) = self.compose.as_mut() {
            c.insert(text);
        }
    }

    /// The file whose comment being written waits without the keyboard: Changes puts a pencil on its row (#168).
    pub(super) fn draft_path(&self, cx: &App) -> Option<String> {
        let c = self.compose.as_ref()?;
        (!c.edit.text().trim().is_empty() && !self.has_keyboard(Input::Compose, cx))
            .then(|| c.path.clone())
    }

    /// The list for the token being typed in the composer, if one is open.
    pub(super) fn compose_completion(
        &self,
        cx: &App,
    ) -> Option<(usize, Vec<super::complete::Choice>)> {
        let c = self.compose.as_ref().filter(|c| !c.preview)?;
        self.completion(c.edit.text(), c.edit.caret(), c.closed, cx)
    }

    /// ↑ ↓ and ⌘← ⌘→ (⇧ selecting) in the composer: by the lines as drawn. Any other key forgets
    /// the x ↑ ↓ keep. Returns whether it took the key.
    pub(super) fn vertical_key(&mut self, key: &gpui::Keystroke, cx: &mut Context<Self>) -> bool {
        use input::Drawn;
        let m = key.modifiers;
        let Some(motion) = input::drawn_motion(key) else {
            self.field_goal = None;
            return false;
        };
        if matches!(motion, Drawn::Start | Drawn::End) {
            self.field_goal = None;
        }
        let Some(layout) = self.field_lines.borrow().clone() else {
            return false;
        };
        let moved = if let Some(compose) = self.compose.as_mut() {
            input::drawn(
                &mut compose.edit,
                &layout,
                &mut self.field_goal,
                motion,
                m.shift,
            )
        } else {
            false
        };
        if moved {
            cx.notify();
        }
        moved
    }

    /// A click in the composer's text: the caret goes there; ⇧ extends the selection, a double
    /// click takes the word; a drag (`drag`) selects.
    fn compose_place(
        &mut self,
        at: usize,
        shift: bool,
        clicks: usize,
        drag: bool,
        lower: bool,
        cx: &mut Context<Self>,
    ) {
        if self.compose.is_none() {
            return;
        }
        // Clicked, it has the keys again, and a filter that had them lets go.
        self.take_keyboard(Input::Compose, cx);
        let Some(compose) = self.compose.as_mut() else {
            return;
        };
        compose.place(at, shift, clicks, drag, lower);
        self.field_goal = None;
        cx.notify();
    }

    /// A markdown helper of the composer's toolbar, on the selection or at the caret.
    pub(super) fn apply_helper(&mut self, helper: Helper, cx: &mut Context<Self>) {
        if self.compose.is_some() {
            self.take_keyboard(Input::Compose, cx);
        }
        let Some(compose) = self.compose.as_mut() else {
            return;
        };
        compose.apply(helper);
        self.review.read(cx).diff_list.remeasure();
        cx.notify();
    }

    pub(super) fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        if self.compose.is_some() {
            self.take_keyboard(Input::Compose, cx);
        }
        if let Some(compose) = self.compose.as_mut() {
            compose.toggle_preview();
            self.review.read(cx).diff_list.remeasure();
            cx.notify();
        }
    }

    /// ⌥M: comments drawn as formatted markdown, or as the text written.
    pub(super) fn toggle_rich_comments(&mut self, cx: &mut Context<Self>) {
        self.settings.rich_comments = !self.settings.rich_comments;
        self.save_settings(cx);
        self.review.read(cx).diff_list.remeasure();
        cx.notify();
    }

    /// A comment on one line, for a folded thread or a tool window: without its markdown when Rich.
    pub(super) fn one_line(&self, body: &str) -> String {
        artoo_ui_kit::format::one_line(body, self.settings.rich_comments)
    }

    /// The project's web address, which relative links in a comment are relative to.
    pub(super) fn link_base(&self) -> Option<&str> {
        self.web.as_ref().map(|w| w.base.as_str())
    }

    /// The *Raw* / *Rich* switch beside a comment that has markdown.
    pub(super) fn rich_switch(&self, id: impl Into<gpui::ElementId>) -> gpui::Stateful<gpui::Div> {
        discussion::rich_switch(id, self.settings.rich_comments)
    }

    /// The composer of a new comment, drawn under its line: my avatar beside the box. A reply is
    /// not drawn here but inside its discussion (`requests::render_request_thread`).
    pub(super) fn render_composer(
        &self,
        indent: f32,
        this: WeakEntity<Self>,
        cx: &gpui::App,
    ) -> gpui::AnyElement {
        let Some(boxed) = self.compose_box(this, cx) else {
            return div().into_any_element();
        };
        // Under 520 px of row the avatar and the indent give way and the box takes the row
        // (Design shell rounds 4 and 6, #129).
        let row = self.widths.width("composer-row");
        let roomy = row == 0. || row >= 520.;
        div()
            .relative()
            .w_full()
            .child(self.widths.measure("composer-row"))
            .pl(px(if roomy { indent } else { 8. }))
            .pr(px(if roomy { 12. } else { 8. }))
            .py(px(6.))
            .child(
                div()
                    // The box at most 760 px, as drawn, so a wide diff reaches L.
                    .max_w(px(800.))
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .children(roomy.then(|| discussion::avatar(&self.me, true, 28.).mt(px(3.))))
                    .child(boxed),
            )
            .into_any_element()
    }

    /// The new comment's input: the one input component (variant D) with who sees it on its edge.
    pub(super) fn compose_box(&self, this: WeakEntity<Self>, cx: &gpui::App) -> Option<gpui::Div> {
        let compose = self.compose.as_ref()?;
        let audience = match compose.target {
            Target::Request(iid) => format!("Everyone on !{iid}"),
            // The chat it will go to, by its page's branch (#77).
            Target::Agent => match self.route_of(self.page_branch(cx).as_deref(), cx) {
                Some(chat) if !chat.title.is_empty() => format!("To: {}", chat.title),
                Some(chat) => format!("To: {}", chat.agent),
                None => "Your agent".to_owned(),
            },
            _ => "Your agent".to_owned(),
        };
        let look = FootLook {
            audience,
            switchable: self.compose_request(cx).is_some(),
            narrow: self.compose_narrow(cx),
            room: self.compose_room(cx),
            drafts: self.agent_drafts().len(),
        };
        let on = this.clone();
        let (chip, tail) = compose.foot(
            look,
            std::rc::Rc::new(move |event, cx| {
                on.update(cx, |this, cx| this.foot_event(event, cx)).ok();
            }),
        );
        let placeholder = match compose.target {
            Target::Agent => "Note for your agent",
            _ => "Comment",
        };
        let input = self.open_input(
            compose,
            compose.source().color(),
            placeholder,
            Some(chip),
            &[Helper::Bold, Helper::Code, Helper::Link, Helper::Bullet],
            tail,
            this,
            cx,
        );
        Some(compose.with_lines(input))
    }

    /// A click on a new comment's foot (`Compose::foot`).
    fn foot_event(&mut self, event: FootEvent, cx: &mut Context<Self>) {
        use super::menu::{Act, Entry};
        let drafts = self.agent_drafts().len();
        let send_label = if drafts > 0 {
            format!("Send all {}", drafts + 1)
        } else {
            "Send".to_owned()
        };
        match event {
            FootEvent::Cancel => self.cancel_comment(cx),
            FootEvent::Switch => self.toggle_target(cx),
            FootEvent::AddToReview(iid) => self.post_comment(iid, true, cx),
            FootEvent::SendAll => self.add_and_send(false, cx),
            FootEvent::Add => self.add_comment(cx),
            // ▾: Send this only, the chat it goes to, Copy as markdown (#50 F).
            FootEvent::More(at) => self.open_menu(
                at,
                vec![
                    vec![
                        Entry::new("Send this only", Act::AgentSendOnly).key("⌥⌘↵"),
                        Entry::new("Choose the chat…", Act::AgentChooseChat),
                    ],
                    vec![Entry::new("Copy as markdown", Act::AgentCopy)],
                ],
                cx,
            ),
            // At S, what folded: into the primary's own ▾.
            FootEvent::Folded(at) => {
                let more = match self.compose.as_ref().map(|c| c.target) {
                    Some(Target::Agent) => vec![
                        vec![
                            Entry::new(send_label, Act::AgentSendAll).key("⇧⌘↵"),
                            Entry::new("Send this only", Act::AgentSendOnly).key("⌥⌘↵"),
                            Entry::new("Choose the chat…", Act::AgentChooseChat),
                        ],
                        vec![Entry::new("Copy as markdown", Act::AgentCopy)],
                    ],
                    Some(Target::Request(iid)) => vec![vec![
                        Entry::new("Add to review", Act::AddToReview(iid)).key("⇧⌘↵"),
                    ]],
                    _ => Vec::new(),
                };
                self.open_menu(at, more, cx)
            }
        }
    }

    /// A reply: the discussion's own one-line field, grown inside its outline (variant D).
    pub(super) fn reply_editor(&self, this: WeakEntity<Self>, cx: &gpui::App) -> Option<gpui::Div> {
        let compose = self.compose.as_ref()?;
        let on = this.clone();
        let (lead, tail, placeholder) = compose.reply_foot(
            self.compose_room(cx),
            self.inbox,
            std::rc::Rc::new(move |event, cx| {
                on.update(cx, |this, cx| this.foot_event(event, cx)).ok();
            }),
        );
        Some(self.open_input(
            compose,
            compose.source().color(),
            placeholder,
            lead,
            &[
                Helper::Bold,
                Helper::Italic,
                Helper::Code,
                Helper::Link,
                Helper::Quote,
                Helper::Bullet,
                Helper::CodeBlock,
            ],
            tail,
            this,
            cx,
        ))
    }

    /// The one input component, open (`Compose::render_input`), its presses answered here: the keyboard taken, the
    /// caret placed, the folded helpers' menu.
    #[allow(clippy::too_many_arguments)]
    fn open_input(
        &self,
        compose: &Compose,
        tone: gpui::Rgba,
        placeholder: &'static str,
        lead: Option<gpui::AnyElement>,
        tools: &[Helper],
        tail: Vec<gpui::AnyElement>,
        this: WeakEntity<Self>,
        cx: &gpui::App,
    ) -> gpui::Div {
        let live = self
            .compose
            .as_ref()
            .is_some_and(|c| std::ptr::eq(c, compose));
        let look = Look {
            tone,
            placeholder,
            lead,
            tools: tools.to_vec(),
            tail,
            folded: self.compose_room(cx) == Room::S,
            // Without the keyboard it shows no caret (#168).
            focused: self.has_keyboard(Input::Compose, cx),
            lines: Some(self.field_lines.clone()),
            completion: self
                .compose_completion(cx)
                .filter(|_| live)
                .map(|(_, choices)| super::complete::render_list(&choices, compose.pick)),
            // Its own width decides its buttons' (`compose_narrow`).
            measure: self.widths.measure("composer").into_any_element(),
            link_base: self.link_base().map(str::to_owned),
        };
        let keys = this.clone();
        let act: ComposeActs = std::rc::Rc::new(move |act, cx| {
            this.update(cx, |this, cx| match act {
                ComposeAct::Helper(h) => this.apply_helper(h, cx),
                ComposeAct::Preview => this.toggle_preview(cx),
                ComposeAct::Format { at, tools } => {
                    let preview = this.compose.as_ref().is_some_and(|c| c.preview);
                    this.open_menu(at, format_menu(&tools, preview), cx)
                }
                ComposeAct::Place {
                    at,
                    shift,
                    clicks,
                    drag,
                    lower,
                } => this.compose_place(at, shift, clicks, drag, lower, cx),
            })
            .ok();
        });
        typing::input_box_of(compose.render_input(look, act, cx), Input::Compose, keys)
    }
}

/// The folded helpers' menu: `tools`, then Preview, on when `preview`.
fn format_menu(tools: &[Helper], preview: bool) -> Vec<Vec<super::menu::Entry>> {
    use super::menu::{Act, Entry};
    let helpers: Vec<Entry> = tools
        .iter()
        .map(|&h| {
            let (_, tip) = markdown::helper_look(h);
            let (name, key) = tip.split_once("  ").unwrap_or((tip, ""));
            let name = name.split(" — ").next().unwrap_or(name);
            Entry::new(name, Act::ComposeHelper(h)).key(key)
        })
        .collect();
    vec![
        helpers,
        vec![
            Entry::new("Preview", Act::ComposePreview)
                .key("⌘⇧P")
                .checked(preview),
        ],
    ]
}
