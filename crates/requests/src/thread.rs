//! A discussion of the request open, drawn under its line: a frame with a bar that folds it, the notes on a
//! timeline — their reactions, the hover bar, Edit and Delete on one's own —, and the reply row (Reply… and Resolve,
//! or the reply's box); and a pending comment (`render_draft`). On its way to the host one is faded and takes no
//! action until it has an id (#139). What the tab keeps comes in its look; a click asks it an event.

use crate::reactions::Hovered;
use crate::requests::{Acts, Requests};
use crate::waiting::{FADED, is_temp, is_temp_thread};
use artoo_core::markdown as text;
use artoo_ui_kit::discussion::{self, Kind, Source, icon_button};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip};
use artoo_ui_kit::markdown;
use artoo_ui_kit::{ROW_RADIUS, keys, px, theme, words};
use gpui::{AnyElement, App, ClickEvent, Entity, FontWeight, SharedString, div, prelude::*};

pub use artoo_requests_api::{DraftEvent, DraftLook, OnDraft, OnThread, ThreadEvent, ThreadLook};

/// Discussion `ix` of the request open, as `look` says, `requests` its entity.
pub fn render_thread(
    requests: &Entity<Requests>,
    ix: usize,
    look: ThreadLook,
    acts: Acts,
    on: OnThread,
    cx: &App,
) -> AnyElement {
    // A picture clicked opens the request's images whole (#211).
    let look = ThreadLook {
        selecting: crate::images::viewing(requests, look.selecting, cx),
        ..look
    };
    let rq = requests.read(cx);
    let Some(t) = rq.threads.get(ix) else {
        return div().into_any_element();
    };
    let ThreadLook {
        indent,
        open,
        focused,
        tight,
        ..
    } = look;
    // On its way to the host: faded, and nothing to act on until it has an id (#139).
    let waits = rq.thread_waits(&t.id);
    let sending = is_temp_thread(&t.id);
    let can_act = look.target && !sending;
    let history = rq.render_history(ix, look.latest, acts.clone());
    let first = &t.notes[0];
    let author = rq
        .current
        .and_then(|iid| rq.find(iid))
        .map(|m| m.author.clone());
    let rich_notes = t.notes.iter().any(|n| markdown::has_markdown(&n.body));
    let (fold, raw) = (on.clone(), on.clone());
    let bar = discussion::bar(("thread-bar", ix), open)
        .child(discussion::source_pill(Source::GitLab))
        .children((!open).then(|| {
            div()
                .flex_none()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(first.author.clone())
        }))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .cut_in_tip()
                .when(open, |s| {
                    s.child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::text())
                                    .child("Discussion"),
                            )
                            .child(format!("· {}", plural(t.notes.len(), "comment"))),
                    )
                })
                .when(!open, |s| s.child(format::one_line(&first.body, look.rich))),
        )
        .children((open && rich_notes).then(|| {
            discussion::rich_switch(("thread-rich", ix), look.rich).on_click(move |_, _, cx| {
                cx.stop_propagation();
                raw(ThreadEvent::ToggleRich, cx);
            })
        }))
        .children(t.resolvable.then(|| {
            discussion::state(t.resolved).when(waits.is_some() && !sending, |s| s.opacity(FADED))
        }))
        .children(
            waits
                .filter(|_| !sending)
                .map(|word| div().flex_none().text_color(theme::muted()).child(word)),
        )
        .tooltip_show_delay(hover::LONG)
        // Folded, a long hover reads its start and how many replied (X59).
        .tooltip({
            let said: SharedString = if open {
                "Collapse  O".into()
            } else {
                let text = text::plain(&first.body);
                let start = match text.char_indices().nth(280) {
                    Some((at, _)) => format!("{}…", &text[..at]),
                    None => text,
                };
                let replies = match t.notes.len() - 1 {
                    0 => "no replies".to_owned(),
                    n => plural(n, "reply"),
                };
                format!("{start}\n{} · {replies}\nExpand  O", first.author).into()
            };
            move |_, cx| cx.new(|_| hover::TipOwned(said.clone())).into()
        })
        // A double click folds or opens it once, as one click does.
        .on_click(move |e: &ClickEvent, _, cx| {
            if e.click_count() > 1 {
                return;
            }
            fold(ThreadEvent::Fold(!open), cx);
        });
    // Replying: the timeline runs on from the last note to my avatar.
    let replying = look.replying.is_some();
    let last = t.notes.len() - 1;
    let mut editing = look.editing;
    let notes: Vec<AnyElement> = t
        .notes
        .iter()
        .enumerate()
        .map(|(n, note)| {
            let edit_box = editing
                .take_if(|(id, _)| *id == note.id)
                .map(|(_, edit_box)| edit_box);
            let own = rq.login.as_deref() == Some(note.login.as_str());
            // Waiting on the host: faded, its time saying what for; a new one takes no actions.
            let (time, note_waits) = rq.note_time(note.id, note.created);
            let can_act = can_act && !is_temp(note.id);
            let editable = can_act && !look.composing && own && !note_waits;
            let pills = rq.reaction_pills(note.id, look.marks, acts.clone());
            let at = Hovered {
                thread: ix,
                note: note.id,
                author: note.login.clone(),
                hovered: look.hovered == Some(note.id),
                github: look.github,
            };
            let bar = can_act
                .then(|| rq.hover_bar(at, requests.downgrade(), acts.clone()))
                .flatten();
            let note_id = note.id;
            let note_el = discussion::note(
                &note.author,
                own,
                (author.as_deref() == Some(note.author.as_str())).then_some("Author"),
                time,
                div()
                    .child(match edit_box {
                        Some(edit_box) => edit_box,
                        None => markdown::body(
                            &note.body,
                            look.rich,
                            format!("md-note-{ix}-{n}"),
                            look.link_base,
                            Some(look.selecting.clone()),
                            cx,
                        ),
                    })
                    .children(pills)
                    .into_any_element(),
                n == last && !replying,
                (bar.is_some() || editable).then(|| {
                    // `e` edits the last of one's notes in the discussion with the keyboard.
                    let key = (focused && look.mine == Some(note_id)).then_some("e");
                    let edit = on.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .children(bar)
                        .children(editable.then(|| {
                            icon_button(
                                ("edit-note", note_id as usize),
                                "pencil",
                                "Edit",
                                key,
                                None,
                                Kind::Quiet,
                                tight,
                            )
                            .h(px(22.))
                            .px(px(6.))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                edit(ThreadEvent::Edit(note_id), cx);
                            })
                        }))
                        // One's own note goes from GitLab too, after a confirm (#76).
                        .children(editable.then(|| {
                            delete_button(note_id, look.confirm == Some(note_id), tight, on.clone())
                        }))
                        .into_any_element()
                }),
                (!own).then(|| ((look.card)(&note.author, &note.login), note.created)),
            );
            // Hovering a note shows its reaction bar.
            let hover = on.clone();
            div()
                .id(("note-hover", note_id as usize))
                .when(note_waits && !sending, |s| s.opacity(FADED))
                .child(note_el)
                .on_hover(move |hovered: &bool, _, cx| {
                    hover(
                        ThreadEvent::Hover {
                            note: note_id,
                            on: *hovered,
                        },
                        cx,
                    )
                })
                .into_any_element()
        })
        .collect();
    // Replying: the box opens where "Reply…" was, inside the discussion.
    // The reply row: my avatar on the timeline's column, then the one input — closed, one line
    // with Resolve beside it; open, grown inside its own outline.
    let my_avatar = || {
        div()
            .w(px(28.))
            .flex()
            .flex_none()
            .justify_center()
            .pt(px(5.))
            .child(discussion::avatar(look.me, true, 24.))
    };
    let reply_row = match look.replying {
        Some(editor) => editor.map(|editor| {
            div()
                .flex()
                .items_start()
                .gap(px(12.))
                .child(my_avatar())
                .child(editor)
        }),
        None => can_act.then(|| {
            let (reply, resolve) = (on.clone(), on.clone());
            div()
                .flex()
                .items_start()
                .gap(px(12.))
                .child(my_avatar())
                .child(
                    div()
                        .id(("thread-reply", ix))
                        .flex_1()
                        .h(px(34.))
                        .pl(px(10.))
                        .pr(px(6.))
                        .flex()
                        .items_center()
                        .rounded(px(8.))
                        .border_1()
                        .border_color(theme::selected())
                        .bg(theme::base())
                        .text_color(theme::muted())
                        .cursor_text()
                        .hover(|s| s.border_color(theme::muted()))
                        .child(div().flex_1().child("Reply…"))
                        .children((!tight).then(|| keys::caps("r", keys::On::Plain)))
                        .on_click(move |_, _, cx| reply(ThreadEvent::Reply, cx)),
                )
                .children(t.resolvable.then(|| {
                    let (icon, label) = if t.resolved {
                        ("rotate-ccw", "Reopen")
                    } else {
                        ("check", "Resolve")
                    };
                    icon_button(
                        ("thread-resolve", ix),
                        icon,
                        label,
                        Some("x"),
                        None,
                        Kind::Plain,
                        tight,
                    )
                    .h(px(34.))
                    .on_click(move |_, _, cx| resolve(ThreadEvent::Resolve, cx))
                }))
        }),
    };
    let pick = on;
    div()
        .w_full()
        .pl(px(indent))
        .pr(px(12.))
        .py(px(6.))
        .child(
            discussion::frame(Source::GitLab, focused)
                .id(("thread", ix))
                .when(sending, |s| s.opacity(FADED))
                .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                    pick(ThreadEvent::Pick, cx)
                })
                .child(bar)
                // Its notes open and fold in size and alpha (#56, 10; #110).
                // Built only while it shows: a folded one draws no notes.
                .children(
                    (open || look.moving)
                        .then(|| {
                            (look.fold)(
                                format!("thread-body-{ix}"),
                                &format!("thread-{ix}"),
                                open,
                                div()
                                    .flex()
                                    .flex_col()
                                    .children(history)
                                    .child(div().flex().flex_col().children(notes))
                                    .children(reply_row)
                                    .into_any_element(),
                            )
                        })
                        .flatten(),
                ),
        )
        .into_any_element()
}

/// Delete on one's own note `note`; `asking`, "Delete on GitLab?" with Delete and Cancel (#76).
fn delete_button(note: u64, asking: bool, tight: bool, on: OnThread) -> AnyElement {
    if !asking {
        return icon_button(
            ("delete-note", note as usize),
            "trash",
            "Delete",
            None,
            None,
            Kind::Quiet,
            tight,
        )
        .h(px(22.))
        .px(px(6.))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            on(ThreadEvent::AskDelete(note), cx);
        })
        .into_any_element();
    }
    let (yes, no) = (on.clone(), on);
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .child(
            div()
                .text_color(theme::removed())
                .child(words::say("Delete on GitLab?").into_owned()),
        )
        .child(
            icon_button(
                ("delete-note-yes", note as usize),
                "trash",
                "Delete",
                None,
                None,
                Kind::Danger,
                false,
            )
            .h(px(22.))
            .px(px(6.))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                yes(ThreadEvent::Delete(note), cx);
            }),
        )
        .child(
            div()
                .id(("delete-note-no", note as usize))
                .px(px(6.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(ROW_RADIUS))
                .text_color(theme::muted())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child("Cancel")
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    no(ThreadEvent::KeepNote, cx);
                }),
        )
        .into_any_element()
}

/// Pending comment `ix` of the request open: a dashed frame, only its author sees it until the review is
/// submitted.
pub fn render_draft(
    rq: &Requests,
    ix: usize,
    look: DraftLook,
    on: OnDraft,
    cx: &App,
) -> AnyElement {
    let Some(d) = rq.drafts.get(ix) else {
        return div().into_any_element();
    };
    let id = d.id;
    let (delete, raw, send) = (on.clone(), on.clone(), on);
    // Waiting on the host: faded, saying what for, nothing to act on (#139).
    let waits = rq.draft_waits(id);
    let acts = waits.is_none() && !is_temp(id);
    let one = look.one;
    // GitHub publishes a pending comment only with its review: there it says so (#138).
    let send = icon_button(
        ("draft-send", ix),
        "send",
        "Send this now",
        None,
        Some(if one {
            "Publishes this comment alone; the others stay pending"
        } else {
            "GitHub sends a pending comment only with its review: Submit review sends it"
        }),
        Kind::Plain,
        look.tight,
    )
    .h(px(24.))
    .px(px(8.))
    .map(|b| {
        if one {
            b.on_click(move |_, _, cx| send(DraftEvent::Send, cx))
        } else {
            b.opacity(0.45).cursor_default()
        }
    });
    let bar = discussion::bar(("draft-bar", ix), true)
        .cursor_default()
        .child(discussion::source_pill(Source::Pending))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                // A reply says whose discussion it goes into (#166).
                .children(
                    rq.draft_thread(d)
                        .and_then(|t| rq.threads[t].notes.first())
                        .map(|n| hover::cut("draft-reply", format!("· a reply to {}", n.author))),
                )
                // An agent's says who wrote it (#150).
                .children(
                    look.agent
                        .map(|agent| hover::cut("draft-agent", format!("· {agent} wrote it"))),
                ),
        )
        .children(markdown::has_markdown(&d.body).then(|| {
            discussion::rich_switch(("draft-rich", ix), look.rich)
                .on_click(move |_, _, cx| raw(DraftEvent::ToggleRich, cx))
        }))
        .children(acts.then_some(send))
        .children(acts.then(|| {
            icon_button(
                ("draft-delete", ix),
                "trash",
                "Delete",
                None,
                None,
                Kind::Danger,
                look.tight,
            )
            .h(px(24.))
            .px(px(8.))
            .on_click(move |_, _, cx| delete(DraftEvent::Delete, cx))
        }));
    div()
        .w_full()
        .pl(px(look.indent))
        .pr(px(12.))
        .py(px(6.))
        .child(
            discussion::frame(Source::Pending, look.focused)
                .when(waits.is_some(), |s| s.opacity(FADED))
                .child(bar)
                .child(div().child(discussion::note(
                    look.me,
                    true,
                    None,
                    waits.unwrap_or_default().to_owned(),
                    markdown::body(
                        &d.body,
                        look.rich,
                        format!("md-draft-{ix}"),
                        look.link_base,
                        Some(look.selecting),
                        cx,
                    ),
                    true,
                    None,
                    None,
                ))),
        )
        .into_any_element()
}
