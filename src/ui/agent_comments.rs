//! The agents' comments on lines (#10, #46, #50 G, #76): added from the composer, sent to a chat or copied for
//! one, an agent's form answered, a purple note on a discussion, deleted, saved on this Mac (`crate::review`),
//! placed in a file against its text, and drawn under their line as a thread.

use super::composer::{Change, Compose, Target, edit_button};
use super::discussion::{self, Kind, Source};
use super::markdown;
use super::px;
use super::rows::FileData;
use super::typing::Input;
use super::{Workspace, theme};
use crate::review::{self, Comment, Place, State};
use crate::textedit::TextEdit;
use gpui::{App, ClipboardItem, Context, WeakEntity, div, prelude::*};

impl Workspace {
    /// Where `comment` sits in the open file now (`None` when it belongs to another file).
    pub(super) fn place_in_open_file(&self, comment: &Comment, cx: &App) -> Option<Place> {
        self.place_in_file(
            comment,
            &self.review.read(cx).current_path()?,
            self.review.read(cx).data.as_ref()?,
        )
    }

    /// Where `comment` sits in file `path` built as `data`: by its line's text, else, its line
    /// changed since, where that line went (`agent_lines`); nowhere once archived (#76).
    fn place_in_file(&self, comment: &Comment, path: &str, data: &FileData) -> Option<Place> {
        if comment.archived {
            return None;
        }
        match place_in(comment, path, data)? {
            Place::Outdated => Some(
                self.agent_lines
                    .get(&comment.id)
                    .map_or(Place::Outdated, |&n| Place::Line(n)),
            ),
            found => Some(found),
        }
    }

    /// Follows the line of every agent comment whose text no longer matches, from the commit it
    /// was written on to the diff shown, in the background (#46).
    pub(super) fn map_agent_comments(&mut self, cx: &mut Context<Self>) {
        self.agent_lines.clear();
        let (Some(root), Some(head), Some(diff)) = (
            self.root.clone(),
            self.review.read(cx).shown_head(),
            self.review.read(cx).diff.as_ref(),
        ) else {
            return;
        };
        let paths: Vec<String> = diff.files.iter().map(|f| f.path().to_owned()).collect();
        let asks: Vec<(u64, String, String, u32)> = self
            .comments(cx)
            .iter()
            .filter(|c| !c.old && paths.contains(&c.path))
            .filter_map(|c| {
                let commit = c.commit.clone()?;
                (!head.to_string().starts_with(&commit))
                    .then(|| (c.id, commit, c.path.clone(), c.line))
            })
            .collect();
        if asks.is_empty() {
            return;
        }
        self.agent_task = Some(cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let Ok(repo) = crate::git::Repo::open(&root) else {
                        return std::collections::HashMap::new();
                    };
                    asks.into_iter()
                        .filter_map(|(id, commit, path, line)| {
                            let from = git2::Oid::from_str(&commit).ok()?;
                            match repo.follow_line(from, head, &path, line) {
                                crate::git::Fate::Same(n) | crate::git::Fate::Changed(n, _) => {
                                    Some((id, n))
                                }
                                crate::git::Fate::Gone => None,
                            }
                        })
                        .collect()
                })
                .await;
            this.update(cx, |this, cx| {
                this.agent_lines = found;
                this.refresh_rows(cx);
                cx.notify();
            })
            .ok();
        }));
    }

    /// Whether folder `path` has a note for the agent not yet resolved or put away (#97).
    pub(super) fn folder_note_waits(&self, path: &str, cx: &App) -> bool {
        self.comments(cx).iter().any(|c| {
            c.path.strip_suffix('/') == Some(path) && !c.archived && c.state != State::Resolved
        })
    }

    /// Adds the agent comment being written to the drafts, then sends it alone or with all.
    pub(super) fn add_and_send(&mut self, only: bool, cx: &mut Context<Self>) {
        let Some(new) = self.add_agent_comment(cx) else {
            return;
        };
        let ids = if only {
            vec![new]
        } else {
            self.agent_drafts(cx)
        };
        self.send_agent(ids, cx);
    }

    /// Adds the agent comment being written; its id, when it was added.
    fn add_agent_comment(&mut self, cx: &mut Context<Self>) -> Option<u64> {
        let before = self.comments(cx).iter().map(|c| c.id).max().unwrap_or(0);
        self.add_comment(cx);
        self.comments(cx)
            .iter()
            .map(|c| c.id)
            .max()
            .filter(|&id| id > before)
    }

    /// 🤖 on a discussion's hover bar: the purple note for the agent on it, written in its reply
    /// field (#50 G, as ⇥ there).
    pub(super) fn start_thread_note(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.start_reply(ix, cx);
        if let Some(c) = self.compose.as_mut()
            && c.target == Target::Reply(ix)
        {
            c.target = Target::ThreadNote(ix);
        }
        cx.notify();
    }

    /// ▾ → Choose the chat…: adds it and asks where it goes.
    pub(super) fn add_and_pick_chat(&mut self, cx: &mut Context<Self>) {
        if let Some(new) = self.add_agent_comment(cx) {
            self.open_agent_pick_for(vec![new], cx);
        }
    }

    /// ▾ → Copy as markdown: adds it and copies every open comment for an agent.
    pub(super) fn add_and_copy(&mut self, cx: &mut Context<Self>) {
        if self.add_agent_comment(cx).is_some() {
            self.copy_for_agent(cx);
        }
    }

    /// The note answering the agent on comment `id` (⇥ Add a note under its form).
    pub(super) fn start_answer(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(c) = self.comments(cx).iter().find(|c| c.id == id) else {
            return;
        };
        let note = c
            .answer
            .as_ref()
            .map(|a| a.note.clone())
            .unwrap_or_default();
        self.compose = Some(Compose {
            path: c.path.clone(),
            old: c.old,
            first: None,
            line: c.line,
            code: c.code.clone(),
            edit: TextEdit::new(note),
            at: String::new(),
            target: Target::Answer(id),
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

    /// Text's own value on the agent's form of comment `id`, in a field of its own.
    pub(super) fn start_answer_value(&mut self, id: u64, cx: &mut Context<Self>) {
        let value = self
            .comments(cx)
            .iter()
            .find(|c| c.id == id)
            .and_then(|c| c.answer.as_ref())
            .map(|a| a.value.clone())
            .unwrap_or_default();
        self.start_answer(id, cx);
        if let Some(c) = self.compose.as_mut() {
            c.target = Target::AnswerValue(id);
            c.edit = TextEdit::new(value);
        }
    }

    /// The typed value replaces any suggestion picked; it goes to the drafts like a pick.
    pub(super) fn save_answer_value(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(compose) = self.compose.take() else {
            return;
        };
        self.change_comment(id, cx, |c| {
            let a = c.answer.get_or_insert_with(Default::default);
            a.value = compose.edit.text().trim().to_owned();
            if !a.value.is_empty() {
                a.picks.clear();
                a.your_call = false;
            }
            c.state = State::Draft;
        });
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// The answer's note goes to the drafts with the comment, for the next send.
    pub(super) fn save_answer_note(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(compose) = self.compose.take() else {
            return;
        };
        self.change_comment(id, cx, |c| {
            let a = c.answer.get_or_insert_with(Default::default);
            a.note = compose.edit.text().trim().to_owned();
            c.state = State::Draft;
        });
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
        // Written in the Inbox: it goes to the chat now (#116).
        if self.inbox && self.zone != super::zones::Zone::Tool {
            self.inbox_answered(id, cx);
        }
    }

    /// A purple note on a GitLab / GitHub discussion: a comment for the agent on its line, with
    /// the discussion to read; never posted (#50 G).
    pub(super) fn add_thread_note(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(compose) = self.compose.take() else {
            return;
        };
        let body = compose.edit.text().trim().to_owned();
        let Some(t) = self.requests.read(cx).threads.get(ix) else {
            return;
        };
        if body.is_empty() {
            self.compose = Some(compose);
            return;
        }
        let thread = t
            .notes
            .iter()
            .map(|n| format!("> {}: {}", n.author, n.body.replace('\n', " ")))
            .collect::<Vec<_>>()
            .join("\n");
        let code = self
            .review
            .read(cx)
            .data
            .as_ref()
            .map(|d| d.side(compose.old).line(compose.line).0.to_owned())
            .unwrap_or_default();
        let id = self.agent_comments.read(cx).next_id();
        let comment = Comment {
            id,
            path: compose.path,
            old: compose.old,
            line: compose.line,
            code,
            body,
            at: compose.at,
            thread: Some(thread),
            ..Default::default()
        };
        self.change_comments(cx, |list| list.push(comment));
        self.save_comments(cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    pub(super) fn delete_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.change_comments(cx, |list| list.retain(|c| c.id != id));
        self.save_comments(cx);
        self.copied = false;
        self.refresh_rows(cx);
        cx.notify();
    }

    pub(super) fn save_comments(&self, cx: &App) {
        self.agent_comments.read(cx).save();
    }

    /// The reviewer's comments to agents, as they are now (`AgentComments`).
    pub(super) fn comments<'a>(&self, cx: &'a App) -> &'a [Comment] {
        &self.agent_comments.read(cx).list
    }

    /// Comment `id` changed by `f`, when it is one; not saved.
    pub(super) fn change_comment<R>(
        &mut self,
        id: u64,
        cx: &mut App,
        f: impl FnOnce(&mut Comment) -> R,
    ) -> Option<R> {
        self.agent_comments
            .update(cx, |c, _| c.list.iter_mut().find(|c| c.id == id).map(f))
    }

    /// The comments changed by `f`; not saved.
    pub(super) fn change_comments<R>(
        &mut self,
        cx: &mut App,
        f: impl FnOnce(&mut Vec<Comment>) -> R,
    ) -> R {
        self.agent_comments.update(cx, |c, _| f(&mut c.list))
    }

    /// The comments as markdown, copied for pasting into an agent.
    pub(super) fn copy_for_agent(&mut self, cx: &mut Context<Self>) {
        let repo = self
            .root
            .as_ref()
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // What the agent still has to do: not the comments it did or the reviewer resolved.
        let comments: Vec<(Comment, Place)> = self
            .comments(cx)
            .iter()
            .filter(|c| c.open())
            .map(|c| (c.clone(), self.place_of(c, cx)))
            .collect();
        cx.write_to_clipboard(ClipboardItem::new_string(review::export(&repo, &comments)));
        self.copied = true;
        cx.notify();
    }

    /// A comment's place for the export and the list: exact in the open file, the written line
    /// elsewhere (the other files are not loaded).
    fn place_of(&self, comment: &Comment, cx: &App) -> Place {
        self.place_in_open_file(comment, cx)
            .unwrap_or(Place::Line(comment.line))
    }

    /// An agent note, drawn under its line: a purple discussion frame with one note.
    pub(super) fn render_thread(
        &self,
        id: u64,
        indent: f32,
        this: WeakEntity<Self>,
        cx: &gpui::App,
    ) -> gpui::AnyElement {
        let Some(comment) = self.comments(cx).iter().find(|c| c.id == id) else {
            return div().into_any_element();
        };
        // A resolved comment starts folded; a click flips either.
        let resolved = comment.state == State::Resolved;
        let open = self.toggled_agent.contains(&id) == resolved;
        let selecting = self.selecting(this.clone());
        let (fold, delete, raw) = (this.clone(), this.clone(), this.clone());
        let (resolve, reopen, editing, asking) = (this.clone(), this.clone(), this.clone(), this);
        // A chat by the name it was given, else its title; without one, by its agent.
        let chat = |s: &crate::review::Sent| {
            let title = crate::labels::chat(&s.session).unwrap_or_else(|| s.title.clone());
            if title.is_empty() {
                self.chats
                    .read(cx)
                    .list
                    .iter()
                    .find(|c| c.session == s.session)
                    .map_or_else(|| "its chat".to_owned(), |c| c.agent.clone())
            } else {
                format!("“{title}”")
            }
        };
        let status = match (&comment.state, &comment.sent) {
            (State::Draft, _) => "draft".to_owned(),
            (State::Sent, Some(s)) if s.session.is_empty() => "sent to a new chat".to_owned(),
            (State::Sent, Some(s)) if s.delivered => format!("with {}", chat(s)),
            (State::Sent, Some(s)) => format!("sent to {} · waits for it to listen", chat(s)),
            (State::Sent, None) => "sent".to_owned(),
            (State::Addressed, _) => "the agent says it is done — check it".to_owned(),
            (State::Resolved, _) => "resolved".to_owned(),
        };
        let bar = discussion::bar(("agent-bar", id), open)
            .child(discussion::source_pill(Source::Agent))
            .child(
                super::hover::cut(
                    "summary",
                    if open {
                        format!("· {status}")
                    } else {
                        self.one_line(&comment.body)
                    },
                )
                .flex_1(),
            )
            .children((open && markdown::has_markdown(&comment.body)).then(|| {
                self.rich_switch(("agent-rich", id))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        raw.update(cx, |this, cx| this.toggle_rich_comments(cx))
                            .ok();
                    })
            }))
            .children((comment.state == State::Addressed).then(|| {
                discussion::button(
                    ("resolve-agent", id),
                    "Resolve",
                    None,
                    Kind::Primary(theme::agent()),
                )
                .h(px(24.))
                .px(px(8.))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    resolve
                        .update(cx, |this, cx| this.resolve_agent_comment(id, cx))
                        .ok();
                })
            }))
            .children(
                matches!(comment.state, State::Addressed | State::Resolved).then(|| {
                    discussion::icon_button(
                        ("reopen-agent", id),
                        "rotate-ccw",
                        "Reopen",
                        None,
                        None,
                        Kind::Plain,
                        self.review.read(cx).tight,
                    )
                    .h(px(24.))
                    .px(px(8.))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        reopen
                            .update(cx, |this, cx| this.reopen_agent_comment(id, cx))
                            .ok();
                    })
                }),
            )
            .child(
                discussion::icon_button(
                    ("delete-comment", id),
                    "trash",
                    "Delete",
                    None,
                    None,
                    Kind::Danger,
                    self.review.read(cx).tight,
                )
                .h(px(24.))
                .px(px(8.))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    delete
                        .update(cx, |this, cx| this.delete_comment(id, cx))
                        .ok();
                }),
            )
            // A double click folds or opens it once, as one click does.
            .on_click(move |e: &gpui::ClickEvent, _, cx| {
                if e.click_count() > 1 {
                    return;
                }
                fold.update(cx, |this, cx| {
                    this.mark_motion(&format!("agent-thread-{id}"), cx);
                    if !this.toggled_agent.remove(&id) {
                        this.toggled_agent.insert(id);
                    }
                    this.refresh_rows(cx);
                    cx.notify();
                })
                .ok();
            });
        let change = Change::Agent(id);
        let edit_box = self.edit_box(change, editing.clone(), cx);
        let body = discussion::note(
            &self.me,
            true,
            None,
            comment.at.clone(),
            match edit_box {
                Some(edit_box) => edit_box.into_any_element(),
                None => markdown::body(
                    &comment.body,
                    self.settings.rich_comments,
                    format!("md-agent-{id}"),
                    None,
                    Some(selecting.clone()),
                    cx,
                ),
            },
            comment.replies.is_empty(),
            (self.compose.is_none()).then(|| {
                edit_button(
                    ("edit-agent", id),
                    None,
                    self.review.read(cx).tight,
                    change,
                    editing,
                )
            }),
            None,
        );
        // The last question is answered under it while the comment is open; an earlier one, or
        // the last once it is done, shows its form as it was answered.
        let last = comment.replies.len().saturating_sub(1);
        let asking_now = comment.open();
        let replies = comment.replies.iter().enumerate().map(|(ix, reply)| {
            let record = reply
                .form
                .as_ref()
                .filter(|_| ix != last || !asking_now)
                .map(|form| {
                    let answered = if ix == last {
                        comment.answer.as_ref().or(reply.answered.as_ref())
                    } else {
                        reply.answered.as_ref()
                    };
                    self.render_form_record(form, answered)
                });
            div()
                .flex()
                .flex_col()
                .child(discussion::note(
                    "Agent",
                    false,
                    Some("agent"),
                    super::format::ago(reply.at),
                    markdown::body(
                        &reply.body,
                        self.settings.rich_comments,
                        format!("md-agent-{id}-{ix}"),
                        None,
                        Some(selecting.clone()),
                        cx,
                    ),
                    ix + 1 == comment.replies.len(),
                    None,
                    None,
                ))
                .children(record)
        });
        // The agent asked with a form: answered here, the pick going to the drafts (#10 C).
        let form = self.render_agent_form(id, asking.clone(), cx);
        let answering = self.compose.as_ref().is_some_and(
            |c| matches!(c.target, Target::Answer(i) | Target::AnswerValue(i) if i == id),
        );
        let answer_box = answering
            .then(|| self.reply_editor(asking, cx))
            .flatten()
            .map(|editor| div().ml(px(52.)).mr(px(12.)).mb(px(10.)).child(editor));
        let on_thread = comment.thread.is_some().then(|| {
            div()
                .mx(px(12.))
                .mb(px(6.))
                .text_size(px(11.))
                .text_color(theme::faint())
                .child("🔒 On a discussion")
        });
        let notes = div()
            .flex()
            .flex_col()
            .children(on_thread)
            .child(body)
            .children(replies)
            .children(form)
            .children(answer_box);
        div()
            .w_full()
            .pl(px(indent))
            .pr(px(12.))
            .py(px(6.))
            .child(
                discussion::frame(Source::Agent, false)
                    .child(bar)
                    // Its notes open and fold in size and alpha (#110).
                    .children(self.folding(
                        format!("agent-thread-body-{id}"),
                        &format!("agent-thread-{id}"),
                        open,
                        notes,
                    )),
            )
            .into_any_element()
    }
}

/// Where `comment` sits in file `path`, built as `data` (`None` when it belongs to another file).
fn place_in(comment: &Comment, path: &str, data: &FileData) -> Option<Place> {
    if comment.path != path {
        return None;
    }
    let side = data.side(comment.old);
    Some(review::place(comment, side.count(), |n| side.line(n).0))
}
