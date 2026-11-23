//! The Agents tab's sessions (#220): what each of the project's sessions is doing, in the order the list shows them —
//! those waiting for the reviewer first, then by their last message, the ended ones last and only for the day they
//! ended. Pure: the window draws what this says.

use crate::agents::Chat;
use artoo_agents_api::comment::{Comment, FormKind, Kind, State, kind_of};
use artoo_agents_api::conversation::{Entry, Message, Said, conversation, working_on};
use artoo_ui_kit::format;

/// What a session is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Doing {
    /// It asked a question and waits for the answer.
    Asks(String),
    /// It proposed a plan and waits for the OK.
    WaitsOk,
    /// Working on this many of the reviewer's comments.
    Working(usize),
    /// Listening, nothing to do; `last` the words it ended on, if any.
    Done,
    /// Gone, with nothing waiting.
    Ended,
}

/// One session of the list.
#[derive(Clone, Debug)]
pub struct Session {
    pub chat: Chat,
    pub doing: Doing,
    /// Its last message, seconds since the epoch (else when it was last heard).
    pub last: i64,
    /// The words it last said, for the row.
    pub said: String,
}

impl Session {
    /// Waiting for the reviewer.
    pub fn waits(&self) -> bool {
        matches!(self.doing, Doing::Asks(_) | Doing::WaitsOk)
    }

    pub fn ended(&self) -> bool {
        self.doing == Doing::Ended
    }

    /// The row's second line.
    pub fn line(&self) -> String {
        match &self.doing {
            Doing::Asks(q) if !q.is_empty() => format!("Asks you · {q}"),
            Doing::Asks(_) => "Asks you".to_owned(),
            Doing::WaitsOk => "Waits for your OK".to_owned(),
            Doing::Working(1) => "Working on your comment".to_owned(),
            Doing::Working(n) => format!("Working on {n} of your comments"),
            Doing::Done if !self.said.is_empty() => self.said.clone(),
            Doing::Done => format!("Done {}", format::clock(self.last)),
            Doing::Ended => format!("Ended {}", format::clock(self.last)),
        }
    }

    /// The head's words under the name: the agent and how it is.
    pub fn head_line(&self) -> String {
        let how = match &self.doing {
            Doing::Asks(_) => "asks you".to_owned(),
            Doing::WaitsOk => "waits for your OK".to_owned(),
            Doing::Working(1) => "working on your comment".to_owned(),
            Doing::Working(_) => "working on your comments".to_owned(),
            Doing::Done => format!("done {}", format::clock(self.last)),
            Doing::Ended => format!("ended {}", format::clock(self.last)),
        };
        format!("{} · {how}", self.chat.agent)
    }
}

/// The session's name: its title, else its agent.
pub fn name(chat: &Chat) -> String {
    if chat.title.is_empty() {
        chat.agent.clone()
    } else {
        chat.title.clone()
    }
}

/// The sessions to list, in order: waiting, the rest by last message, then the ones that ended today. `now` is
/// seconds since the epoch.
pub fn sessions(
    chats: &[Chat],
    comments: &[Comment],
    messages: &[Message],
    said: &[Said],
    now: i64,
) -> Vec<Session> {
    let alive = |s: &str| chats.iter().any(|c| c.session == s && c.listening());
    let offset = format::local_offset(now);
    let midnight = (now + offset).div_euclid(86400) * 86400 - offset;
    let mut out: Vec<Session> = chats
        .iter()
        .map(|chat| {
            let talk = conversation(comments, messages, said, &chat.session);
            let last = talk.iter().map(Entry::at).max().unwrap_or(chat.beat);
            let words = talk
                .iter()
                .rev()
                .find_map(|e| match e {
                    Entry::Claude { text, .. } if !text.is_empty() => Some(text.as_str()),
                    _ => None,
                })
                .map(|t| format::one_line(t, false))
                .unwrap_or_default();
            let asks = comments
                .iter()
                .filter(|c| {
                    c.sent.as_ref().is_some_and(|s| s.session == chat.session)
                        && kind_of(c, &alive) == Kind::Asks
                })
                .filter_map(|c| c.replies.last())
                .next_back();
            let working = working_on(comments, &chat.session).len();
            let doing = match asks {
                Some(r) if r.form.as_ref().is_some_and(|f| f.kind == FormKind::Plan) => {
                    Doing::WaitsOk
                }
                Some(r) => Doing::Asks(format::one_line(&r.body, false)),
                None if working > 0 => Doing::Working(working),
                None if chat.listening() => Doing::Done,
                None => Doing::Ended,
            };
            Session {
                chat: chat.clone(),
                doing,
                last,
                said: words,
            }
        })
        .filter(|s| !s.ended() || s.last >= midnight)
        .collect();
    out.sort_by_key(|s| (s.ended(), !s.waits(), std::cmp::Reverse(s.last)));
    out
}

/// How many comments wait to be sent (the `Not sent` count): drafts for the agent.
pub fn not_sent(comments: &[Comment]) -> usize {
    comments
        .iter()
        .filter(|c| c.state == State::Draft && !c.archived)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_agents_api::comment::{Form, Reply, Sent};

    fn chat(session: &str, beat: i64) -> Chat {
        Chat {
            session: session.into(),
            agent: "Claude Code".into(),
            title: session.into(),
            beat,
            watcher: 1,
            ..Default::default()
        }
    }

    fn sent(id: u64, session: &str, at: i64) -> Comment {
        Comment {
            id,
            state: State::Sent,
            sent: Some(Sent {
                session: session.into(),
                at,
                delivered: true,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn those_waiting_come_first_then_by_last_message_and_the_ended_last() {
        let now = crate::agents::now();
        let chats = [
            chat("quiet", now),
            chat("busy", now),
            chat("asks", now),
            chat("gone", now - 3600),
        ];
        let mut asked = sent(3, "asks", now - 300);
        asked.replies.push(Reply {
            body: "Which error text?".into(),
            form: Some(Form::default()),
            at: now - 290,
            ..Default::default()
        });
        let comments = [
            sent(1, "busy", now - 10),
            asked,
            sent(2, "quiet", now - 900),
        ];
        let comments = {
            let mut c = comments.to_vec();
            c[2].replies.push(Reply {
                body: "Done".into(),
                at: now - 800,
                ..Default::default()
            });
            c[2].state = State::Addressed;
            c
        };
        let list = sessions(&chats, &comments, &[], &[], now);
        let names: Vec<_> = list.iter().map(|s| s.chat.session.as_str()).collect();
        assert_eq!(names, ["asks", "busy", "quiet", "gone"]);
        assert_eq!(list[0].doing, Doing::Asks("Which error text?".into()));
        assert_eq!(list[1].doing, Doing::Working(1));
        assert_eq!(list[2].doing, Doing::Done);
        assert!(list[3].ended());
    }

    #[test]
    fn a_plan_waits_for_the_ok_and_an_ended_session_leaves_the_next_day() {
        let now = crate::agents::now();
        let mut planned = sent(1, "p", now - 60);
        planned.replies.push(Reply {
            form: Some(Form {
                kind: FormKind::Plan,
                ..Default::default()
            }),
            at: now - 30,
            ..Default::default()
        });
        let list = sessions(&[chat("p", now)], &[planned], &[], &[], now);
        assert_eq!(list[0].doing, Doing::WaitsOk);
        assert_eq!(list[0].head_line(), "Claude Code · waits for your OK");
        let old = chat("old", now - 3 * 86400);
        assert!(sessions(&[old], &[], &[], &[], now).is_empty());
    }

    #[test]
    fn not_sent_counts_the_drafts_alone() {
        let draft = Comment::default();
        let archived = Comment {
            archived: true,
            ..Default::default()
        };
        assert_eq!(not_sent(&[draft, archived, sent(1, "a", 1)]), 1);
    }
}
