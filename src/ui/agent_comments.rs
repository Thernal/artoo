//! The agents' comments on lines (#10, #46, #50 G, #76) as the tab writes them: added from the composer, sent to a
//! chat or copied for one, an agent's form answered, a purple note on a discussion, deleted, and drawn under their
//! line as a thread. What is done to them — kept, placed in a file against its text, their lines followed — is the
//! agents' (`Agents`); the tab hears them changed (`on_comments`).

use super::composer::{Change, Compose, Target, edit_button};
use super::discussion::{self, Kind, Source};
use super::markdown;
use super::px;
use super::typing::Input;
use super::{Workspace, theme};
use crate::comment::{Comment, Place, State};
use crate::textedit::TextEdit;
use gpui::{App, ClipboardItem, Context, WeakEntity, div, prelude::*};

impl Workspace {
    /// Where `comment` sits in the open file now (`None` when it belongs to another file).
    pub(super) fn place_in_open_file(&self, comment: &Comment, cx: &App) -> Option<Place> {
        let (path, data) = (self.current_path(cx)?, self.diff_data(cx)?);
        self.agents.place(comment, &path, &data, cx)
    }

    /// Follows the line of every agent comment on the diff shown whose text no longer matches, from the commit it
    /// was written on to the one shown, in the background (#46, `Agents::follow_comments`).
    pub(super) fn map_agent_comments(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<String> = (self.shown_diff(cx))
            .map(|d| d.files.iter().map(|f| f.path().to_owned()).collect())
            .unwrap_or_default();
        let head = self.rv().shown_head(cx).map(|h| h.to_string());
        self.agents
            .follow_comments(self.root.clone(), head, paths, cx);
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
            attachments: Vec::new(),
            emoji: None,
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
        self.agents.answer_value(id, compose.edit.text(), cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// The answer's note goes to the drafts with the comment, for the next send.
    pub(super) fn save_answer_note(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.compose_waits() {
            return;
        }
        let Some(compose) = self.compose.take() else {
            return;
        };
        self.agents.answer_note(id, compose.edit.text(), cx);
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
        if self.compose_waits() {
            return;
        }
        let Some(compose) = self.compose.take() else {
            return;
        };
        let body = compose.edit.text().trim().to_owned();
        let Some(t) = self.request_thread(ix, cx) else {
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
            .diff_data(cx)
            .map(|d| d.side(compose.old).line(compose.line).0.to_owned())
            .unwrap_or_default();
        let comment = Comment {
            path: compose.path,
            old: compose.old,
            line: compose.line,
            code,
            body,
            at: compose.at,
            thread: Some(thread),
            ..Default::default()
        };
        self.agents.add_comment(comment, cx);
        self.refresh_rows(cx);
        cx.notify();
    }

    /// Comment `id` gone; its line drawn again when the agents say so (`on_comments`).
    pub(super) fn delete_comment(&mut self, id: u64, cx: &mut Context<Self>) {
        self.agents.delete_comment(id, cx);
        self.copied = false;
        cx.notify();
    }

    /// The comment's own way out (#199): a draft deleted, `Comment deleted · Undo`; one sent withdrawn; one read
    /// archived.
    pub(super) fn erase_comment(&mut self, id: u64, erase: Erase, cx: &mut Context<Self>) {
        match erase {
            Erase::Delete => {
                let kept = self.comments(cx).iter().find(|c| c.id == id).cloned();
                self.delete_comment(id, cx);
                if let Some(comment) = kept {
                    let agents = self.agents.clone();
                    let undo = std::rc::Rc::new(move |cx: &mut App| {
                        agents.restore_comment(comment.clone(), cx)
                    });
                    self.toasts
                        .update(cx, |t, cx| t.show_with("Comment deleted", "Undo", undo, cx));
                }
            }
            Erase::Withdraw => self.agents.withdraw(id, cx),
            Erase::Archive => self.agents.archive(id, true, cx),
        }
        self.copied = false;
        cx.notify();
    }

    /// The reviewer's comments to agents, as they are now (`Agents::comments`).
    pub(super) fn comments<'a>(&self, cx: &'a App) -> &'a [Comment] {
        self.agents.comments(cx)
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
        let app: &App = cx;
        let text = (self.agents).export_comments(&repo, &|c| self.place_of(c, app), app);
        cx.write_to_clipboard(ClipboardItem::new_string(text));
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
        self.thread_card(id, indent, false, this, cx)
    }

    /// The thread of comment `id` as the Inbox previews its answer: the card's head says who, where and what
    /// the agent did (`Claude  LoginViewModel.kt:51  Claude asks you`).
    pub(super) fn render_answer_thread(
        &self,
        id: u64,
        this: WeakEntity<Self>,
        cx: &gpui::App,
    ) -> gpui::AnyElement {
        self.thread_card(id, 0., true, this, cx)
    }

    /// The agent comment `c` is for or from, by its name: `Claude`, not `Claude Code`; `None` when no chat of
    /// the tab's is the one it went to.
    pub(super) fn agent_name_of(
        &self,
        c: &crate::comment::Comment,
        cx: &gpui::App,
    ) -> Option<String> {
        c.sent
            .as_ref()
            .and_then(|s| {
                self.agents
                    .chats(cx)
                    .iter()
                    .find(|chat| chat.session == s.session)
            })
            .map(|chat| chat.agent.trim_end_matches(" Code").to_owned())
            .filter(|n| !n.is_empty())
    }

    fn thread_card(
        &self,
        id: u64,
        indent: f32,
        answer: bool,
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
        let (editing, asking) = (this.clone(), this);
        let draft = comment.state == State::Draft;
        // The agent it is for, by its name.
        let to = self
            .agent_name_of(comment, cx)
            .unwrap_or_else(|| "Claude".to_owned());
        // The far end: when it went, or that it did not, with its own send (A8, K1a).
        let sent_at = comment
            .sent
            .as_ref()
            .map(|s| if s.at > 0 { s.at } else { comment.written })
            .filter(|t| *t > 0)
            .map(artoo_ui_kit::format::clock);
        let erase_kind = match comment.state {
            State::Draft => Erase::Delete,
            State::Sent if comment.replies.is_empty() => Erase::Withdraw,
            _ => Erase::Archive,
        };
        let marked = open && markdown::has_markdown(&comment.body);
        let hover = gpui::SharedString::from(format!("agent-head-{id}"));
        let (erase, send, erase_foot) = (delete.clone(), delete.clone(), delete.clone());
        let head = div()
            .id(("agent-bar", id))
            .group(hover.clone())
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(28.))
            .cursor_pointer()
            // The agent's mark, a tinted square, as the design draws it (the owner, 2026-11-26).
            .child(
                div()
                    .id(gpui::SharedString::from(format!("agent-for-{id}")))
                    .flex_none()
                    .size(px(22.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::Rgba {
                        a: 0.18,
                        ..theme::agent()
                    })
                    .child(
                        super::icons::icon("bot")
                            .size(px(13.))
                            .text_color(theme::agent()),
                    ),
            )
            // A draft: a dashed circle, not sent yet; once sent, to whom.
            .child(if draft && !answer {
                div()
                    .id(("agent-draft-mark", id))
                    .flex_none()
                    .child(
                        super::icons::icon("circle-dashed")
                            .size(px(14.))
                            .text_color(theme::faint()),
                    )
                    .tooltip_show_delay(super::hover::LONG)
                    .tooltip({
                        let tip = format!("A draft for {to}, not sent yet");
                        move |_, cx| {
                            cx.new(|_| super::hover::TipOwned(tip.clone().into()))
                                .into()
                        }
                    })
                    .into_any_element()
            } else {
                div()
                    .flex_none()
                    .text_size(px(13.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(if answer {
                        to.clone()
                    } else {
                        format!("To {to}")
                    })
                    .into_any_element()
            })
            .children(answer.then(|| {
                div()
                    .min_w_0()
                    .font_family(theme::code_font())
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(super::hover::cut("answer-place", comment.reads()))
            }))
            .children(answer.then(|| {
                div()
                    .flex_none()
                    .ml(px(8.))
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(theme::agent())
                    .child(format!("{to} {}", artoo_agents_api::comment::verb(comment)))
            }))
            .child(if open || answer {
                div().flex_1().into_any_element()
            } else {
                super::hover::cut("summary", self.one_line(&comment.body))
                    .flex_1()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .into_any_element()
            })
            // Edit and Delete only on hover; the markdown switch with them when the words have markdown.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .invisible()
                    .when(!(draft && open), |s| s.group_hover(hover, |s| s.visible()))
                    .children(marked.then(|| {
                        self.rich_switch(("agent-rich", id))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                raw.update(cx, |this, cx| this.toggle_rich_comments(cx))
                                    .ok();
                            })
                    }))
                    .child(edit_button(
                        ("edit-agent", id),
                        None,
                        true,
                        Change::Agent(id),
                        editing.clone(),
                    ))
                    .child(
                        discussion::icon_button(
                            ("delete-comment", id),
                            "trash",
                            match erase_kind {
                                Erase::Delete => "Delete",
                                Erase::Withdraw => "Take back",
                                Erase::Archive => "Archive",
                            },
                            None,
                            None,
                            Kind::Quiet,
                            true,
                        )
                        .h(px(22.))
                        .px(px(6.))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            erase
                                .update(cx, |this, cx| this.erase_comment(id, erase_kind, cx))
                                .ok();
                        }),
                    ),
            )
            .child(if draft && open {
                div().into_any_element()
            } else if draft {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        discussion::icon_button(
                            ("send-agent", id),
                            "send",
                            "Send",
                            None,
                            None,
                            Kind::Primary(theme::agent()),
                            false,
                        )
                        .h(px(22.))
                        .tooltip_show_delay(super::hover::LONG)
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                super::hover::TipOwned("Send only this one to Claude".into())
                            })
                            .into()
                        })
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            send.update(cx, |this, cx| this.send_agent(vec![id], cx))
                                .ok();
                        }),
                    )
                    .into_any_element()
            } else if answer {
                // The preview's head already says when.
                div().into_any_element()
            } else {
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_right()
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(match sent_at {
                        Some(t) => format!("sent {t}"),
                        None => "sent".to_owned(),
                    })
                    .into_any_element()
            })
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
        let editing_now = edit_box.is_some();
        let body = div().pb(px(6.)).child(match edit_box {
            Some(edit_box) => edit_box.into_any_element(),
            // A draft is yours: who and when over its words, no face — the card's purple rail says it is for the agent
            // (the owner, 2026-11-26).
            None if draft => discussion::faceless_note(
                "You",
                if comment.written > 0 {
                    super::format::short_ago(comment.written)
                } else {
                    String::new()
                },
                markdown::body(
                    &comment.body,
                    self.settings.rich_comments,
                    format!("md-agent-{id}"),
                    None,
                    Some(selecting.clone()),
                    cx,
                ),
            )
            .into_any_element(),
            None => markdown::body(
                &comment.body,
                self.settings.rich_comments,
                format!("md-agent-{id}"),
                None,
                Some(selecting.clone()),
                cx,
            ),
        });
        // A draft's foot: Edit at the left, Delete and Send at the right (the design's note to an agent).
        let foot = (draft && !answer && !editing_now).then(|| {
            let (erase, send) = (erase_foot.clone(), erase_foot.clone());
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .px(px(12.))
                .pb(px(10.))
                .child(
                    div()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme::island_border())
                        .child(edit_button(
                            ("edit-agent-foot", id),
                            None,
                            true,
                            Change::Agent(id),
                            editing.clone(),
                        )),
                )
                .child(div().flex_1())
                .child(
                    discussion::icon_button(
                        ("delete-agent-foot", id),
                        "x",
                        "Delete",
                        None,
                        None,
                        Kind::Quiet,
                        true,
                    )
                    .h(px(26.))
                    .border_1()
                    .border_color(theme::island_border())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        erase
                            .update(cx, |this, cx| this.erase_comment(id, Erase::Delete, cx))
                            .ok();
                    }),
                )
                .child(
                    discussion::icon_button(
                        ("send-agent-foot", id),
                        "send",
                        "Send",
                        None,
                        Some("Send only this one"),
                        Kind::Primary(theme::agent()),
                        false,
                    )
                    .h(px(26.))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        send.update(cx, |this, cx| this.send_agent(vec![id], cx))
                            .ok();
                    }),
                )
        });
        // The last question is answered under it while the comment is open; an earlier one, or
        // the last once it is done, shows its form as it was answered.
        let last = comment.replies.len().saturating_sub(1);
        let asking_now = comment.open();
        // Claude's name wherever the chat has one; `Agent` only when it has none.
        let agent_name = comment
            .sent
            .as_ref()
            .and_then(|s| s.title.split(" · ").next().map(str::to_owned))
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Agent".to_owned());
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
                    artoo_agents_api::view::record(form, answered, cx)
                });
            div()
                .flex()
                .flex_col()
                // The agent's answer: its bar, no face (the owner, 2026-11-26).
                .child(discussion::agent_note(
                    &agent_name,
                    super::format::short_ago(reply.at),
                    markdown::body(
                        &reply.body,
                        self.settings.rich_comments,
                        format!("md-agent-{id}-{ix}"),
                        None,
                        Some(selecting.clone()),
                        cx,
                    ),
                ))
                .children(record)
        });
        // The agent asked with a form: answered here, the pick going to the drafts (#10 C).
        let form = self.agent_form(comment, asking.clone(), cx);
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
            .children(answer_box)
            .children(foot);
        div()
            .w_full()
            .pl(px(indent))
            .pr(px(12.))
            .py(px(6.))
            .child(
                discussion::frame(Source::Agent, false)
                    // Where `Open in Changes` took the reviewer: lit for a moment (#220).
                    .when(self.lit == Some(id), |d| {
                        let accent = theme::accent();
                        d.border_2()
                            .border_color(accent)
                            .shadow(vec![gpui::BoxShadow {
                                color: gpui::Rgba { a: 0.45, ..accent }.into(),
                                offset: gpui::point(px(0.), px(0.)),
                                blur_radius: px(16.),
                                spread_radius: px(2.),
                                inset: false,
                            }])
                    })
                    .child(head)
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

/// How a comment to an agent goes away, by its state (#199).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Erase {
    /// Not sent yet: deleted, with Undo.
    Delete,
    /// Sent, the agent not answered yet: taken back.
    Withdraw,
    /// The agent read it (answered, done, resolved): put away, kept.
    Archive,
}
