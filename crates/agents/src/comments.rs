//! A tab's comments to agents (#10, #46, #76): the reviewer's line comments of this repository, kept on this Mac
//! (`review::load`, `review::save`), and what the agents answered on them, taken in on the poll's beat; where each
//! is in a file shown, its line followed from the commit it was written on (#46). What a change of them means on
//! screen — the lines' pins, a toast — is the tab's.

use crate::agents::{self, Bus};
use crate::git;
use crate::review::{self, Comment, Place, State};
use artoo_agents_api::comment::{Answered, FormKind, Kind, export, kind_of, next_id};
use artoo_core::rows::FileData;
use gpui::{App, Context, Entity, Task};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// The reviewer's comments to agents in one tab.
#[derive(Default)]
pub struct AgentComments {
    /// Oldest first.
    pub list: Vec<Comment>,
    /// Where each comment whose line changed since it was written is on the head shown, by id (`follow`).
    pub lines: HashMap<u64, u32>,
    /// The lines being followed.
    task: Option<Task<()>>,
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

    /// `comment` added to the drafts as the next one; its id. Not saved.
    pub fn add(&mut self, mut comment: Comment) -> u64 {
        comment.id = next_id(&self.list);
        // When it was written, for its row before the agent answers (#201).
        if comment.written == 0 {
            comment.written = artoo_agents_api::bus::now();
        }
        let id = comment.id;
        self.list.push(comment);
        id
    }

    /// `comment` put in by its id, where it was: a draft deleted brought back (its toast's Undo). Not saved.
    pub fn restore(&mut self, comment: Comment) {
        self.list.push(comment);
        self.list.sort_by_key(|c| c.id);
    }

    /// Comment `id` gone. Whether it was one; not saved.
    pub fn delete(&mut self, id: u64) -> bool {
        let before = self.list.len();
        self.list.retain(|c| c.id != id);
        self.list.len() != before
    }

    /// `value` typed on the agent's form on comment `id`, in a field of its own: it replaces any pick, and goes to the
    /// drafts like one (#10 C). Whether it is one; not saved.
    pub fn answer_value(&mut self, id: u64, value: &str) -> bool {
        self.change(id, |c| {
            let a = c.answer.get_or_insert_with(Default::default);
            a.value = value.trim().to_owned();
            if !a.value.is_empty() {
                a.picks.clear();
                a.your_call = false;
            }
            c.state = State::Draft;
        })
    }

    /// `note` under the agent's form on comment `id`: it goes to the drafts with the comment, for the next send.
    /// Whether it is one; not saved.
    pub fn answer_note(&mut self, id: u64, note: &str) -> bool {
        self.change(id, |c| {
            c.answer.get_or_insert_with(Default::default).note = note.trim().to_owned();
            c.state = State::Draft;
        })
    }

    /// Whether folder `path` has a note for the agent not yet resolved or put away (#97).
    pub fn folder_waits(&self, path: &str) -> bool {
        self.list.iter().any(|c| {
            c.path.strip_suffix('/') == Some(path) && !c.archived && c.state != State::Resolved
        })
    }

    /// Where `comment` sits in file `path` built as `data`: by its line's text, else, its line changed since, where
    /// that line went (`follow`); nowhere once archived (#76) or in another file.
    pub fn place(&self, comment: &Comment, path: &str, data: &FileData) -> Option<Place> {
        let side = data.side(comment.old);
        self.place_in(comment, path, side.count(), |n| side.line(n).0)
    }

    /// `place` in a file of `count` lines, `line` each one's text.
    fn place_in<'a>(
        &self,
        comment: &Comment,
        path: &str,
        count: u32,
        line: impl Fn(u32) -> &'a str,
    ) -> Option<Place> {
        if comment.archived || comment.path != path {
            return None;
        }
        match review::place(comment, count, line) {
            Place::Outdated => Some(
                self.lines
                    .get(&comment.id)
                    .map_or(Place::Outdated, |&n| Place::Line(n)),
            ),
            found => Some(found),
        }
    }

    /// The open ones — not those the agent did or the reviewer resolved — as markdown for an agent, each at its place
    /// `place`; `repo` the repository's name.
    pub fn export(&self, repo: &str, place: impl Fn(&Comment) -> Place) -> String {
        let open: Vec<(Comment, Place)> = (self.list.iter())
            .filter(|c| c.open())
            .map(|c| (c.clone(), place(c)))
            .collect();
        export(repo, &open)
    }

    /// Follows the line of every comment on the new side of files `paths` written on another commit than `head` (the
    /// one shown), from that commit to it, in repository `root`, in the background; whoever observes them told when
    /// they were found.
    pub fn follow(
        &mut self,
        root: Option<PathBuf>,
        head: Option<String>,
        paths: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        self.lines.clear();
        let (Some(root), Some(head)) = (root, head) else {
            return;
        };
        let Ok(to) = git2::Oid::from_str(&head) else {
            return;
        };
        let asks: Vec<(u64, String, String, u32)> = (self.list.iter())
            .filter(|c| !c.old && paths.contains(&c.path))
            .filter_map(|c| {
                let commit = c.commit.clone()?;
                (!head.starts_with(&commit)).then(|| (c.id, commit, c.path.clone(), c.line))
            })
            .collect();
        if asks.is_empty() {
            return;
        }
        self.task = Some(cx.spawn(async move |this, cx| {
            let found = cx
                .background_executor()
                .spawn(async move {
                    let Ok(repo) = git::Repo::open(&root) else {
                        return HashMap::new();
                    };
                    asks.into_iter()
                        .filter_map(|(id, commit, path, line)| {
                            let from = git2::Oid::from_str(&commit).ok()?;
                            match repo.follow_line(from, to, &path, line) {
                                git::Fate::Same(n) | git::Fate::Changed(n, _) => Some((id, n)),
                                git::Fate::Gone => None,
                            }
                        })
                        .collect()
                })
                .await;
            this.update(cx, |this, cx| {
                this.lines = found;
                cx.notify();
            })
            .ok();
        }));
    }

    /// Those an agent answered that wait for the reviewer, for the Inbox — one it asks on, one it says it did, a draft
    /// whose answer was picked again —, `alive` the sessions listening: each one's id, when the agent last wrote, and
    /// the key read once it is looked at (`agent:<id>:<replies>`: a new answer is new again).
    /// `lists` says whether a kind of notice is listed (#196): one it asks on is `agents.asks`, one it did
    /// `agents.done`.
    pub fn inbox_answers(
        &self,
        alive: &dyn Fn(&str) -> bool,
        lists: &dyn Fn(&str) -> bool,
    ) -> Vec<artoo_core::notice::Answer> {
        use artoo_agents_api::notices::{ASKS, DONE};
        (self.list.iter())
            .filter(|c| !c.archived)
            .filter_map(|c| {
                let asks = match kind_of(c, alive) {
                    Kind::Asks => lists(ASKS).then_some(true),
                    Kind::Addressed => lists(DONE).then_some(false),
                    Kind::Draft => {
                        (c.answer.is_some() && !c.replies.is_empty() && lists(ASKS)).then_some(true)
                    }
                    _ => None,
                }?;
                let last = c.replies.last()?;
                Some(artoo_core::notice::Answer {
                    id: c.id,
                    at: last.at,
                    key: format!("agent:{}:{}", c.id, c.replies.len()),
                    asks,
                    text: format!("{}\n{}", c.body, last.body),
                })
            })
            .collect()
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
    fn a_value_typed_replaces_the_picks_and_a_note_goes_with_them() {
        let mut c = asked(FormKind::Many, &[true, true]);
        assert!(c.pick(7, Some(0)));
        assert!(c.answer_note(7, "  and keep the old one  "));
        assert!(c.answer_value(7, " 30 s "));
        let a = c.list[0].answer.clone().unwrap_or_default();
        assert_eq!(
            (a.value.as_str(), a.note.as_str(), a.picks.is_empty()),
            ("30 s", "and keep the old one", true)
        );
        assert!(c.answer_value(7, ""));
        assert!(!c.answer_note(8, "x"), "no such comment");
    }

    #[test]
    fn a_comment_keeps_when_it_was_written() {
        let mut c = AgentComments::default();
        c.add(Comment::default());
        c.add(Comment {
            written: 5,
            ..Default::default()
        });
        assert!(c.list[0].written >= artoo_agents_api::bus::now() - 5);
        assert_eq!(c.list[1].written, 5, "one brought back keeps its own");
    }

    #[test]
    fn a_draft_deleted_comes_back_where_it_was() {
        let mut c = AgentComments::default();
        let (one, two) = (
            c.add(Comment::default()),
            c.add(Comment {
                body: "b".to_owned(),
                ..Default::default()
            }),
        );
        assert_eq!((one, two), (1, 2));
        let was = c.list[0].clone();
        assert!(c.delete(one));
        assert!(!c.delete(one), "gone already");
        c.restore(was);
        assert_eq!(c.list.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn a_comment_is_placed_by_its_text_else_where_its_line_went() {
        let mut c = AgentComments::default();
        let comment = |line: u32, code: &str| Comment {
            id: line.into(),
            path: "f.kt".to_owned(),
            line,
            code: code.to_owned(),
            ..Default::default()
        };
        let text = ["a", "b", "c"];
        let place = |c: &AgentComments, comment: &Comment, path: &str| {
            c.place_in(comment, path, 3, |n| text[n as usize - 1])
        };
        assert_eq!(place(&c, &comment(2, "b"), "f.kt"), Some(Place::Line(2)));
        let gone = comment(3, "changed since");
        assert_eq!(place(&c, &gone, "f.kt"), Some(Place::Outdated));
        c.lines.insert(3, 1);
        assert_eq!(place(&c, &gone, "f.kt"), Some(Place::Line(1)));
        assert_eq!(place(&c, &gone, "g.kt"), None, "another file's");
        let archived = Comment {
            archived: true,
            ..comment(2, "b")
        };
        assert_eq!(place(&c, &archived, "f.kt"), None);
    }

    #[test]
    fn a_folder_waits_until_its_note_is_resolved_or_put_away() {
        let mut c = AgentComments::default();
        c.add(Comment {
            path: "src/ui/".to_owned(),
            ..Default::default()
        });
        assert!(c.folder_waits("src/ui"));
        assert!(!c.folder_waits("src"));
        c.list[0].state = State::Resolved;
        assert!(!c.folder_waits("src/ui"));
    }

    #[test]
    fn only_the_open_ones_are_exported() {
        let mut c = AgentComments::default();
        for (body, state) in [("to do", State::Draft), ("done", State::Resolved)] {
            c.add(Comment {
                path: "f.kt".to_owned(),
                body: body.to_owned(),
                state,
                ..Default::default()
            });
        }
        let out = c.export("app", |c| Place::Line(c.line));
        assert!(out.contains("to do") && !out.contains("done"), "{out}");
    }

    #[test]
    fn the_inbox_lists_what_an_agent_asked_or_did_until_it_is_archived() {
        let mut c = asked(FormKind::One, &[false, false]);
        c.list[0].replies[0].at = 40;
        let listening = |_: &str| true;
        let all = |_: &str| true;
        let answers = c.inbox_answers(&listening, &all);
        assert_eq!(
            (
                answers.len(),
                answers[0].id,
                answers[0].at,
                answers[0].key.as_str()
            ),
            (1, 7, 40, "agent:7:1")
        );
        assert!(answers[0].asks, "it asks you: Asks you's (#197)");
        assert!(c.pick(7, Some(0)), "its answer picked: a draft again");
        assert_eq!(
            c.inbox_answers(&listening, &all).len(),
            1,
            "still waits for the send"
        );
        c.list[0].state = State::Addressed;
        c.list[0].replies.push(Default::default());
        assert!(
            !c.inbox_answers(&listening, &all)[0].asks,
            "done: no longer asks"
        );
        assert_eq!(
            c.inbox_answers(&listening, &all)[0].key,
            "agent:7:2",
            "a new answer is new again"
        );
        // An agent finished, its kind turned off in the Inbox (#196): not listed.
        let no_done = |kind: &str| kind != artoo_agents_api::notices::DONE;
        assert!(c.inbox_answers(&listening, &no_done).is_empty());
        c.list[0].archived = true;
        assert!(c.inbox_answers(&listening, &all).is_empty());
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
