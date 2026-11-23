//! The Not sent view of the Agents tab (#220): every comment written for the agent and not sent, in the order they
//! were written — its place and its words in full, on hover Edit and a send of its own —, under them the field for
//! a message to go with them and `Send all`.

use crate::review::{Comment, State};
use artoo_agents_api::view::{Act, AgentsHost};
use artoo_ui_kit::{ROW_RADIUS, hover, icons, markdown, px, theme};
use gpui::{AnyElement, App, ClickEvent, SharedString, div, prelude::*};
use std::rc::Rc;

/// The comments not sent, in the order they were written (ids only grow).
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

/// Where the sends of the rows go: the open session.
pub struct To {
    pub session: String,
    pub title: String,
    pub host: Option<Rc<dyn AgentsHost>>,
    pub rich: bool,
}

/// A comment's row: its place, Edit and Send only this one on hover, its words under.
fn row(c: &Comment, to: &To, cx: &App) -> AnyElement {
    let id = c.id;
    let group: SharedString = format!("not-sent-{id}").into();
    let (shown, edited, sent) = (to.host.clone(), to.host.clone(), to.host.clone());
    let (session, title) = (to.session.clone(), to.title.clone());
    let icon = |name: &'static str, tip: &'static str, key: u64| {
        div()
            .id(("not-sent-act", key as usize))
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
        .id(("not-sent", id as usize))
        .group(group.clone())
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(14.))
        .py(px(8.))
        .hover(|d| d.bg(theme::hover()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(22.))
                .child(
                    div()
                        .id(("not-sent-place", id as usize))
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::text()))
                        .tooltip_show_delay(hover::LONG)
                        .tooltip(|_, cx| cx.new(|_| hover::Tip("Go to it in Changes")).into())
                        .child(
                            icons::icon("chevron-right")
                                .size(px(12.))
                                .text_color(theme::faint()),
                        )
                        .child(icons::icon("file").size(px(13.)).text_color(theme::muted()))
                        .child(hover::cut(("not-sent-what", id as usize), c.reads()))
                        .on_click(move |_: &ClickEvent, _, cx| {
                            if let Some(host) = &shown {
                                host.act(Act::Show(id), cx);
                            }
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
            format!("not-sent-body-{id}"),
            None,
            None,
            cx,
        )))
        .into_any_element()
}

/// The rows, oldest first; the empty line when nothing waits.
pub fn view(comments: &[Comment], to: &To, cx: &App) -> Vec<AnyElement> {
    let list = drafts(comments);
    if list.is_empty() {
        return vec![
            div()
                .px(px(14.))
                .py(px(10.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child("Nothing waits: comments you write for Claude on lines gather here")
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
    fn not_sent_lists_the_drafts_in_the_order_written() {
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
        assert!(drafts(&all).is_empty(), "nothing is left in Not sent");
        assert_eq!(
            artoo_agents_api::conversation::working_on(&all, "s1"),
            [1, 2, 3]
        );
    }

    #[test]
    fn the_tooltip_names_the_count() {
        assert_eq!(
            send_all_tip(10),
            "Send all 10 to Claude: your message first, then each comment as a message of its own"
        );
    }
}
