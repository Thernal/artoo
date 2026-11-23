//! A session's conversation with the reviewer (#220): what the Agents tab reads in order. It is derived from what
//! is kept — the comments, their `Sent` and `Reply` — plus only what is not: the reviewer's own messages, and
//! what the agent said of its own accord through Artoo's MCP server (`Said`, the developer's decision D1: Artoo's
//! own record, so Codex has the same, rather than Claude Code's transcripts).

use crate::comment::{Answered, Comment, Form, FormKind, State};
use serde::{Deserialize, Serialize};

/// A message of the reviewer to a session, with no comment in it (a send with a message carries its comments' ids).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Unique in the project: the receipts name it.
    pub id: u64,
    /// The chat it was sent to; empty for "whichever chat asks".
    pub session: String,
    pub text: String,
    /// Seconds since the epoch.
    pub at: i64,
    /// The comments sent together with it, in the order they were written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub carried: Vec<u64>,
    /// The chat's watcher handed it over.
    #[serde(default)]
    pub delivered: bool,
    /// The agent read it (`list_comments`).
    #[serde(default)]
    pub read: bool,
}

/// What an agent said without a comment to say it on: a remark, or a status (`Done · 3 files changed`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Said {
    /// Unique, so a line read twice counts once.
    pub uid: String,
    pub session: String,
    pub text: String,
    pub at: i64,
    /// A status line rather than words: drawn centred.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub status: bool,
}

/// What an answer of the agent is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ref {
    /// A comment, by its id.
    Comment(u64),
    /// A plan, on a comment's id.
    Plan(u64),
    /// A question, on a comment's id.
    Question(u64),
}

/// One line of a conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// The reviewer's message; `carried` are the comments sent with it.
    You {
        at: i64,
        text: String,
        carried: Vec<u64>,
    },
    /// A comment sent, as a message of its own.
    Comment { at: i64, id: u64 },
    /// The agent's words: an answer (`on`), or of its own accord (`None`). `form` is what it asked with, `answered`
    /// what the reviewer picked.
    Claude {
        at: i64,
        text: String,
        on: Option<Ref>,
        form: Option<Form>,
        answered: Option<Answered>,
    },
    /// A status line: `Done · 3 files changed`.
    Status { at: i64, text: String },
}

impl Entry {
    pub fn at(&self) -> i64 {
        match self {
            Entry::You { at, .. }
            | Entry::Comment { at, .. }
            | Entry::Claude { at, .. }
            | Entry::Status { at, .. } => *at,
        }
    }

    /// Where it stands among lines of the same second: your message, its comments, then what the agent says.
    fn rank(&self) -> u8 {
        match self {
            Entry::You { .. } => 0,
            Entry::Comment { .. } => 1,
            Entry::Claude { .. } => 2,
            Entry::Status { .. } => 3,
        }
    }
}

/// Whether a comment sent to `to` is `session`'s: its own, and those sent to no chat in particular.
fn is_for(to: &str, session: &str) -> bool {
    to.is_empty() || to == session
}

/// `session`'s conversation, oldest first. A comment sent is a message of its own; a message sent with comments
/// comes before them (its `carried`); an answer carries what it is on.
pub fn conversation(
    comments: &[Comment],
    messages: &[Message],
    said: &[Said],
    session: &str,
) -> Vec<Entry> {
    let mut out = Vec::new();
    for m in messages.iter().filter(|m| is_for(&m.session, session)) {
        out.push(Entry::You {
            at: m.at,
            text: m.text.clone(),
            carried: m.carried.clone(),
        });
    }
    for c in comments {
        let Some(sent) = c.sent.as_ref().filter(|s| is_for(&s.session, session)) else {
            continue;
        };
        let at = if sent.at > 0 { sent.at } else { c.written };
        out.push(Entry::Comment { at, id: c.id });
        let last_form = c.replies.iter().rposition(|r| r.form.is_some());
        for (i, r) in c.replies.iter().enumerate() {
            let on = match r.form.as_ref().map(|f| f.kind) {
                Some(FormKind::Plan) => Ref::Plan(c.id),
                Some(_) => Ref::Question(c.id),
                None => Ref::Comment(c.id),
            };
            // The last question's answer is the comment's current one; an earlier one kept its own.
            let answered = if Some(i) == last_form {
                c.answer.clone().or_else(|| r.answered.clone())
            } else {
                r.answered.clone()
            };
            out.push(Entry::Claude {
                at: r.at,
                text: r.body.clone(),
                on: Some(on),
                form: r.form.clone(),
                answered,
            });
        }
    }
    for s in said.iter().filter(|s| is_for(&s.session, session)) {
        out.push(if s.status {
            Entry::Status {
                at: s.at,
                text: s.text.clone(),
            }
        } else {
            Entry::Claude {
                at: s.at,
                text: s.text.clone(),
                on: None,
                form: None,
                answered: None,
            }
        });
    }
    // Stable: lines of the same second keep the order they were pushed in (comments by id).
    out.sort_by_key(|e| (e.at(), e.rank()));
    out
}

/// The ids of the comments `session` is working on: sent to it, not answered, not marked done.
pub fn working_on(comments: &[Comment], session: &str) -> Vec<u64> {
    comments
        .iter()
        .filter(|c| c.state == State::Sent && c.replies.is_empty())
        .filter(|c| c.sent.as_ref().is_some_and(|s| is_for(&s.session, session)))
        .map(|c| c.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comment::{FormOption, Reply, Sent, send};

    fn comments(n: u64) -> Vec<Comment> {
        (1..=n)
            .map(|id| Comment {
                id,
                path: "a.rs".into(),
                body: format!("Comment {id}"),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn ten_comments_and_a_message_sent_together_read_back_as_eleven_with_your_words_first() {
        let mut all = comments(10);
        let ids: Vec<u64> = (1..=10).collect();
        send(&mut all, &ids, "s1", "Fix login");
        let at = all[0].sent.as_ref().unwrap().at;
        let message = Message {
            id: 1,
            session: "s1".into(),
            text: "Take these in order.".into(),
            at,
            carried: ids.clone(),
            ..Default::default()
        };
        let talk = conversation(&all, &[message], &[], "s1");
        assert_eq!(talk.len(), 11);
        assert!(
            matches!(&talk[0], Entry::You { text, carried, .. } if text.starts_with("Take") && carried.len() == 10)
        );
        let order: Vec<u64> = talk
            .iter()
            .filter_map(|e| match e {
                Entry::Comment { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(order, ids);
        // Another chat reads none of it.
        assert!(conversation(&all, &[], &[], "s2").is_empty());
    }

    #[test]
    fn an_answer_names_what_it_is_on_and_what_was_picked() {
        let mut all = comments(3);
        send(&mut all, &[3], "s1", "t");
        let form = Form {
            kind: FormKind::Plan,
            options: vec![FormOption {
                label: "Step".into(),
                ..Default::default()
            }],
        };
        all[2].replies.push(Reply {
            uid: "r".into(),
            body: "Two ways.".into(),
            form: Some(form),
            at: 9_999_999_999,
            answered: None,
        });
        all[2].answer = Some(Answered {
            picks: vec![0],
            ..Default::default()
        });
        let talk = conversation(&all, &[], &[], "s1");
        let Entry::Claude { on, answered, .. } = &talk[1] else {
            panic!("{talk:?}")
        };
        assert_eq!(*on, Some(Ref::Plan(3)));
        assert_eq!(answered.as_ref().unwrap().picks, vec![0]);
    }

    #[test]
    fn the_agents_own_words_and_status_join_in_time() {
        let said = [
            Said {
                uid: "a".into(),
                session: "s1".into(),
                text: "Starting with the form.".into(),
                at: 50,
                status: false,
            },
            Said {
                uid: "b".into(),
                session: "s1".into(),
                text: "Done · 3 files changed".into(),
                at: 60,
                status: true,
            },
        ];
        let talk = conversation(&[], &[], &said, "s1");
        assert!(matches!(&talk[0], Entry::Claude { on: None, .. }));
        assert!(matches!(&talk[1], Entry::Status { text, .. } if text.starts_with("Done")));
    }

    #[test]
    fn working_on_counts_the_unanswered() {
        let mut all = comments(4);
        send(&mut all, &[1, 2, 3], "s1", "t");
        all[1].replies.push(Reply {
            uid: "r".into(),
            body: "Why?".into(),
            ..Default::default()
        });
        all[2].state = State::Addressed;
        assert_eq!(working_on(&all, "s1"), vec![1]);
        assert!(working_on(&all, "s2").is_empty());
    }

    #[test]
    fn a_store_written_by_2_0_0_still_reads() {
        // No `at` on `sent`, no messages file at all.
        let old = r#"[{"id":1,"path":"a.rs","old":false,"line":3,"code":"x","body":"b","at":"abc",
            "state":"sent","sent":{"session":"s1","title":"t","batch":"b1","delivered":true}}]"#;
        let all: Vec<Comment> = serde_json::from_str(old).unwrap();
        assert_eq!(
            all[0].sent,
            Some(Sent {
                session: "s1".into(),
                title: "t".into(),
                batch: "b1".into(),
                delivered: true,
                at: 0,
            })
        );
        let talk = conversation(&all, &[], &[], "s1");
        assert_eq!(talk.len(), 1);
        let none: Vec<Message> = serde_json::from_str("[]").unwrap();
        assert!(none.is_empty());
        let m: Message =
            serde_json::from_str(r#"{"id":1,"session":"s","text":"hi","at":1}"#).unwrap();
        assert!(!m.delivered && !m.read && m.carried.is_empty());
    }
}
