//! The Drafts view of the Agents tab (#220, #243): every comment written for the agent and not sent, in the order
//! they were written — its place and its words in full, on hover Edit and a send of its own —, under them the field for
//! a message to go with them and `Send all`.

use crate::review::{Comment, State};
use artoo_agents_api::view::{Act, AgentsHost};
use artoo_ui_kit::{ROW_RADIUS, hover, icons, markdown, px, theme};
use gpui::{AnyElement, App, ClickEvent, SharedString, div, prelude::*};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// The drafts, in the order they were written (ids only grow).
pub fn drafts(comments: &[Comment]) -> Vec<&Comment> {
    let mut out: Vec<&Comment> = comments
        .iter()
        .filter(|c| c.state == State::Draft && !c.archived)
        .collect();
    out.sort_by_key(|c| c.id);
    out
}

/// Send all's tooltip, from the number it sends.
pub fn send_all_tip(n: usize) -> String {
    format!("Send all {n} to Claude: your message first, then each comment as a message of its own")
}

/// What a click on a place does: toggles that row's peek.
pub type Peek = Rc<dyn Fn(u64, &mut App)>;

/// Where the sends of the rows go: the open session.
pub struct To {
    pub session: String,
    pub title: String,
    pub host: Option<Rc<dyn AgentsHost>>,
    pub rich: bool,
    /// The session's folder, whose files a peek reads.
    pub folder: PathBuf,
    /// The rows whose place is open (#220): five lines under the words.
    pub peeked: HashSet<u64>,
    /// A click on a row's place opens or closes its peek.
    pub peek: Peek,
}

/// The lines a peek shows: five around the comment's own line from the file, that one marked. Where the file no longer
/// has the comment's line there (gone, moved, another branch's), what the comment kept: up to four lines above and its
/// own.
pub fn peek_lines(c: &Comment, folder: &Path) -> Vec<(u32, String, bool)> {
    let cite = crate::cites::Cite {
        path: c.path.clone(),
        line: c.line,
    };
    let around = crate::cites::around(folder, &cite);
    let still = (around.iter())
        .any(|(_, text, own)| *own && (c.code.is_empty() || text.trim() == c.code.trim()));
    if !c.old && still {
        return around;
    }
    let above = c.above.len().min(4);
    let first = c.line.saturating_sub(above as u32);
    let mut out: Vec<(u32, String, bool)> = c.above[c.above.len() - above..]
        .iter()
        .enumerate()
        .map(|(i, l)| (first + i as u32, l.clone(), false))
        .collect();
    out.push((c.line, c.code.clone(), true));
    out
}

/// The open peek: the lines with their numbers, the comment's own marked, and Open in Changes.
fn peek_card(c: &Comment, folder: &Path, host: Option<Rc<dyn AgentsHost>>) -> AnyElement {
    let id = c.id;
    let lines = peek_lines(c, folder).into_iter().map(|(n, text, own)| {
        div()
            .flex()
            .gap(px(8.))
            .px(px(8.))
            .when(own, |d| d.bg(theme::hover()))
            .font_family(theme::code_font())
            .text_size(px(12.))
            .child(
                div()
                    .w(px(28.))
                    .flex_none()
                    .text_color(theme::faint())
                    .child(n.to_string()),
            )
            .child(
                div()
                    .min_w_0()
                    .text_color(if own { theme::text() } else { theme::muted() })
                    .child(SharedString::from(text)),
            )
    });
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .flex()
                .flex_col()
                .py(px(4.))
                .rounded(px(ROW_RADIUS))
                .border_1()
                .border_color(theme::island_border())
                .children(lines),
        )
        .child(
            div()
                .id(("draft-open", id as usize))
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(12.))
                .text_color(theme::accent())
                .cursor_pointer()
                .hover(|d| d.underline())
                .child("Open in Changes")
                .child(
                    icons::icon("external")
                        .size(px(12.))
                        .text_color(theme::accent()),
                )
                .on_click(move |_: &ClickEvent, _, cx| {
                    if let Some(host) = &host {
                        host.act(Act::Show(id), cx);
                    }
                }),
        )
        .into_any_element()
}

/// A comment's row: its place, Edit and Send only this one on hover, its words under.
fn row(c: &Comment, to: &To, cx: &App) -> AnyElement {
    let id = c.id;
    let group: SharedString = format!("draft-{id}").into();
    let (edited, sent) = (to.host.clone(), to.host.clone());
    let peek = to.peek.clone();
    let open = to.peeked.contains(&id);
    let request = crate::talk::request_of(c, to.host.as_ref(), cx).is_some();
    let (session, title) = (to.session.clone(), to.title.clone());
    let icon = |name: &'static str, tip: &'static str, key: u64| {
        div()
            .id(("draft-act", key as usize))
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(ROW_RADIUS))
            .text_color(theme::muted())
            .cursor_pointer()
            .hover(|d| d.bg(theme::hover()).text_color(theme::text()))
            .tooltip_show_delay(hover::LONG)
            .tooltip(move |_, cx| cx.new(|_| hover::Tip(tip)).into())
            .child(icons::icon(name).size(px(14.)).text_color(theme::muted()))
    };
    div()
        .id(("draft", id as usize))
        .group(group.clone())
        .flex()
        .flex_col()
        .gap(px(4.))
        .pl(px(16.))
        .pr(px(8.))
        .pt(px(8.))
        .pb(px(10.))
        .border_b_1()
        .border_color(theme::island_border())
        .hover(|d| d.bg(theme::hover()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(22.))
                .child(
                    div()
                        .id(("draft-place", id as usize))
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::text()))
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                hover::Tip(if open {
                                    "Hide its lines"
                                } else {
                                    "Show its lines here, under its words"
                                })
                            })
                            .into()
                        })
                        .child(
                            icons::icon(if open {
                                "chevron-down"
                            } else {
                                "chevron-right"
                            })
                            .size(px(12.))
                            .text_color(theme::faint()),
                        )
                        .child(
                            icons::icon(if c.thread.is_some() || request {
                                "pull-request"
                            } else {
                                "file"
                            })
                            .size(px(13.))
                            .text_color(theme::muted()),
                        )
                        .child(hover::cut(("draft-what", id as usize), c.reads()))
                        .on_click(move |_: &ClickEvent, _, cx| {
                            peek(id, cx);
                        }),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .flex()
                        .gap(px(2.))
                        .invisible()
                        .group_hover(group, |d| d.visible())
                        .child(icon("pencil", "Edit", id * 2).on_click(
                            move |_: &ClickEvent, _, cx| {
                                if let Some(host) = &edited {
                                    host.act(Act::Edit(id), cx);
                                }
                            },
                        ))
                        .child(
                            icon("send", "Send only this one to Claude", id * 2 + 1).on_click(
                                move |_: &ClickEvent, _, cx| {
                                    if let Some(host) = &sent {
                                        host.act(
                                            Act::SendTo {
                                                ids: vec![id],
                                                session: session.clone(),
                                                title: title.clone(),
                                            },
                                            cx,
                                        );
                                    }
                                },
                            ),
                        ),
                ),
        )
        .child(div().text_size(px(13.)).child(markdown::body(
            &c.body,
            to.rich,
            format!("draft-body-{id}"),
            None,
            None,
            cx,
        )))
        .children(open.then(|| peek_card(c, &to.folder, to.host.clone())))
        .into_any_element()
}

/// The rows, oldest first; with none, what gathers here and how.
pub fn view(comments: &[Comment], to: &To, cx: &App) -> Vec<AnyElement> {
    let list = drafts(comments);
    if list.is_empty() {
        return vec![
            div()
                .px(px(16.))
                .py(px(32.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .text_center()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("No drafts"),
                )
                .child("A comment you write for Claude on a line waits here until you send it.")
                .into_any_element(),
        ];
    }
    list.into_iter().map(|c| row(c, to, cx)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review;
    use artoo_agents_api::comment::{self, Sent};
    use artoo_agents_api::conversation::{Entry, conversation};

    fn draft(id: u64, body: &str) -> Comment {
        Comment {
            id,
            path: "a/Login.kt".into(),
            line: 10 + id as u32,
            body: body.into(),
            ..Default::default()
        }
    }

    #[test]
    fn drafts_are_listed_in_the_order_written() {
        let mut sent = draft(2, "sent");
        sent.state = State::Sent;
        sent.sent = Some(Sent::default());
        let mut gone = draft(4, "gone");
        gone.archived = true;
        let all = [draft(5, "e"), sent, draft(1, "a"), gone, draft(3, "c")];
        let ids: Vec<u64> = drafts(&all).iter().map(|c| c.id).collect();
        assert_eq!(ids, [1, 3, 5]);
    }

    #[test]
    fn send_all_reads_back_your_words_first_then_each_comment() {
        let mut all = vec![draft(1, "a"), draft(2, "b"), draft(3, "c")];
        let ids: Vec<u64> = drafts(&all).iter().map(|c| c.id).collect();
        let mut kept = Vec::new();
        let mut m = review::next_message(&kept, "s1", "Take the login first.", 100).unwrap();
        m.carried = ids.clone();
        kept.push(m);
        comment::send(&mut all, &ids, "s1", "chat");
        for c in &mut all {
            c.sent.as_mut().unwrap().at = 100;
        }
        let talk = conversation(&all, &kept, &[], "s1");
        assert!(matches!(&talk[0], Entry::You { text, carried, .. }
            if text == "Take the login first." && carried == &ids));
        let order: Vec<u64> = talk
            .iter()
            .filter_map(|e| match e {
                Entry::Comment { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(order, [1, 2, 3], "each comment its own message, as written");
        assert_eq!(talk.len(), 4);
        assert!(drafts(&all).is_empty(), "nothing is left in Drafts");
        assert_eq!(
            artoo_agents_api::conversation::working_on(&all, "s1"),
            [1, 2, 3]
        );
    }

    #[test]
    fn a_peek_marks_the_comments_own_line_under_the_lines_above() {
        let mut c = draft(1, "x");
        c.line = 12;
        c.code = "val c = 3".into();
        c.above = vec![
            "a".into(),
            "b".into(),
            "c".into(),
            "d".into(),
            "e".into(),
            "f".into(),
        ];
        let lines = peek_lines(&c, Path::new("/nonexistent"));
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].0, 8);
        assert_eq!(lines[4], (12, "val c = 3".into(), true));
        assert!(lines[..4].iter().all(|l| !l.2));
    }

    #[test]
    fn a_peek_shows_five_lines_around_from_the_file() {
        let dir = std::env::temp_dir().join(format!("artoo-peek-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.kt"), "1\n2\n3\n4\n5\n6\n7\n").unwrap();
        let mut c = draft(1, "x");
        c.path = "a.kt".into();
        c.line = 4;
        c.code = "4".into();
        let lines = peek_lines(&c, &dir);
        assert_eq!(
            lines.iter().map(|l| l.0).collect::<Vec<_>>(),
            [2, 3, 4, 5, 6]
        );
        assert_eq!(lines.iter().filter(|l| l.2).count(), 1);
        assert!(lines[2].2);
        // The line is not there any more: the comment's own words.
        c.code = "gone".into();
        assert_eq!(peek_lines(&c, &dir), [(4, "gone".to_owned(), true)]);
    }

    #[test]
    fn the_tooltip_names_the_count() {
        assert_eq!(
            send_all_tip(10),
            "Send all 10 to Claude: your message first, then each comment as a message of its own"
        );
    }
}
