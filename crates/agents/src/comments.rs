//! A tab's comments to agents (#10, #46, #76): the reviewer's line comments of this repository, kept on this Mac
//! (`review::load`, `review::save`), and what the agents answered on them, taken in on the poll's beat. What a
//! change of them means on screen — the lines' pins, a toast — is the tab's.

use crate::agents::{self, Bus};
use crate::review::{self, Comment, State};
use artoo_agents_api::comment::{Answered, FormKind};
use gpui::{App, Entity};
use std::path::PathBuf;
use std::time::SystemTime;

/// The reviewer's comments to agents in one tab.
#[derive(Default)]
pub struct AgentComments {
    /// Oldest first.
    pub list: Vec<Comment>,
    /// The project they are kept for.
    root: Option<PathBuf>,
    /// When the agents' answers were last read.
    answers_stamp: Option<SystemTime>,
}

impl AgentComments {
    /// Project `root`'s, read anew; its answers read again on the next beat.
    pub fn load(&mut self, root: PathBuf) {
        self.list = review::load(&root);
        self.root = Some(root);
        self.answers_stamp = None;
    }

    /// Kept for the project, as they are now.
    pub fn save(&self) {
        if let Some(root) = &self.root {
            review::save(root, &self.list);
        }
    }

    /// What the agents answered since the last look, taken in and kept: each one's state and replies as they were,
    /// when anything changed.
    pub fn take_answers(&mut self, bus: &Bus) -> Option<Vec<(u64, State, usize)>> {
        let root = self.root.clone()?;
        let stamp = bus.answers_stamp(&root);
        if stamp.is_none() || stamp == self.answers_stamp {
            return None;
        }
        self.answers_stamp = stamp;
        let before = self
            .list
            .iter()
            .map(|c| (c.id, c.state, c.replies.len()))
            .collect();
        agents::apply(&mut self.list, &bus.answers(&root)).then(|| {
            self.save();
            before
        })
    }

    /// The reviewer's pick on the agent's form on comment `id` — `None` their own call, else option `pick` —: a draft
    /// again, to go with the next send (#10 C). Whether it is one; not saved.
    pub fn pick(&mut self, id: u64, pick: Option<usize>) -> bool {
        let Some(c) = self.list.iter_mut().find(|c| c.id == id) else {
            return false;
        };
        let form = c.replies.last().and_then(|r| r.form.clone());
        let a = c.answer.get_or_insert_with(|| Answered {
            picks: match &form {
                Some(f) if f.kind == FormKind::Order => (0..f.choices().len()).collect(),
                Some(f) if f.kind.many() => (f.choices().iter().enumerate())
                    .filter(|(_, o)| o.on)
                    .map(|(i, _)| i)
                    .collect(),
                _ => Vec::new(),
            },
            ..Default::default()
        });
        match (pick, &form) {
            (None, _) => a.your_call = !a.your_call,
            (Some(i), Some(f)) if f.kind.many() => {
                a.your_call = false;
                match a.picks.iter().position(|p| *p == i) {
                    Some(at) => {
                        a.picks.remove(at);
                    }
                    None => a.picks.push(i),
                }
                a.picks.sort_unstable();
            }
            (Some(i), _) => {
                a.your_call = false;
                a.picks = vec![i];
            }
        }
        c.state = State::Draft;
        true
    }

    /// An Order's item `at` on comment `id` moved one place up (or down). Whether it moved; not saved.
    pub fn move_pick(&mut self, id: u64, at: usize, up: bool) -> bool {
        let Some(c) = self.list.iter_mut().find(|c| c.id == id) else {
            return false;
        };
        let n = (c.replies.last())
            .and_then(|r| r.form.as_ref())
            .map_or(0, |f| f.choices().len());
        let a = c.answer.get_or_insert_with(|| Answered {
            picks: (0..n).collect(),
            ..Default::default()
        });
        let to = if up { at.checked_sub(1) } else { Some(at + 1) };
        let Some(to) = to.filter(|&t| t < a.picks.len()) else {
            return false;
        };
        a.picks.swap(at, to);
        c.state = State::Draft;
        true
    }

    /// A digit on comment `id`'s form: 0 their own call, 1…9 the option drawn with that number (Line: the suggestion
    /// in order); an Order takes none. Whether it took the key; not saved.
    pub fn key_pick(&mut self, id: u64, digit: usize) -> bool {
        if digit == 0 {
            return self.pick(id, None);
        }
        let form = (self.list.iter().find(|c| c.id == id))
            .and_then(|c| c.replies.last())
            .and_then(|r| r.form.clone());
        match form {
            Some(f) if f.kind != FormKind::Order && digit <= f.choices().len() => {
                self.pick(id, Some(digit - 1))
            }
            _ => false,
        }
    }

    /// Comment `id` changed by `f`. Whether it is one; not saved.
    pub fn change(&mut self, id: u64, f: impl FnOnce(&mut Comment)) -> bool {
        self.list.iter_mut().find(|c| c.id == id).map(f).is_some()
    }

    /// Sent and not taken yet (or never taken by a chat gone): a draft again.
    pub fn withdraw(&mut self, id: u64) -> bool {
        self.change(id, |c| {
            c.state = State::Draft;
            c.sent = None;
        })
    }

    /// Every resolved one archived: off the lists and its line.
    pub fn archive_resolved(&mut self) {
        for c in self.list.iter_mut().filter(|c| c.state == State::Resolved) {
            c.archived = true;
        }
    }
}

/// Comments `comments` changed by `f` when it says so: kept, and whoever observes them told (the tab draws their
/// lines again, `Agents::on_comments`). What `f` said.
pub fn change(
    comments: &Entity<AgentComments>,
    cx: &mut App,
    f: impl FnOnce(&mut AgentComments) -> bool,
) -> bool {
    comments.update(cx, |c, cx| {
        let changed = f(c);
        if changed {
            c.save();
            cx.notify();
        }
        changed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_agents_api::comment::{Form, FormOption, Reply};

    fn asked(kind: FormKind, on: &[bool]) -> AgentComments {
        let options = (on.iter())
            .map(|on| FormOption {
                label: "x".to_owned(),
                on: *on,
                ..Default::default()
            })
            .collect();
        AgentComments {
            list: vec![Comment {
                id: 7,
                state: State::Sent,
                replies: vec![Reply {
                    form: Some(Form { kind, options }),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn picks(c: &AgentComments) -> (Vec<usize>, bool, State) {
        let a = c.list[0].answer.clone().unwrap_or_default();
        (a.picks, a.your_call, c.list[0].state)
    }

    #[test]
    fn a_plans_steps_are_picked_from_those_it_proposed() {
        let mut c = asked(FormKind::Plan, &[true, true, false]);
        assert!(c.key_pick(7, 3));
        assert_eq!(picks(&c), (vec![0, 1, 2], false, State::Draft));
        assert!(c.key_pick(7, 1));
        assert_eq!(picks(&c).0, vec![1, 2]);
        assert!(c.key_pick(7, 0));
        assert!(picks(&c).1);
        assert!(!c.key_pick(7, 4), "no fourth step");
        assert!(!c.key_pick(8, 1), "no such comment");
    }

    #[test]
    fn one_choice_is_one_pick_and_an_order_takes_no_digit() {
        let mut c = asked(FormKind::One, &[false, false]);
        assert!(c.key_pick(7, 2));
        assert!(c.key_pick(7, 1));
        assert_eq!(picks(&c).0, vec![0]);
        let mut order = asked(FormKind::Order, &[false, false, false]);
        assert!(!order.key_pick(7, 1));
        assert!(order.move_pick(7, 0, false));
        assert_eq!(picks(&order).0, vec![1, 0, 2]);
        assert!(!order.move_pick(7, 0, true), "the first goes no higher");
    }

    #[test]
    fn a_withdrawn_comment_is_a_draft_with_no_chat() {
        let mut c = asked(FormKind::One, &[]);
        c.list[0].sent = Some(Default::default());
        assert!(c.withdraw(7));
        assert_eq!(
            (c.list[0].state, c.list[0].sent.is_none()),
            (State::Draft, true)
        );
        c.list[0].state = State::Resolved;
        c.archive_resolved();
        assert!(c.list[0].archived);
    }
}
