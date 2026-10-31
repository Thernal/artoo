//! An agent's notes on the lines of its changes (#151): why this way, what it left alone, what to
//! look at closely, with a summary of the whole. An agent writes them with `annotate_changes` (or
//! `open_changes`); the app keeps them per project on this Mac, as comments for an agent are, and
//! never sends them to GitLab or GitHub. A note sits on a line of the new side, found again by the
//! line's text, and follows it from the commit it was written on (#46).

use crate::git;
use artoo_review_api::diff::{Diff, Header};
use gpui::{Context, Task};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What changes the notes are on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum On {
    /// The uncommitted changes.
    WorkingTree,
    /// A branch, a tag or a commit, by the name the agent gave and the commit it was at.
    Head { name: String, commit: String },
}

/// One note, on line `line` of `path` as the changes have it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    /// Given by the app when it takes the notes; 0 until then.
    #[serde(default)]
    pub id: u64,
    pub path: String,
    pub line: u32,
    /// The line's text when it was written: where it is found again.
    pub code: String,
    pub body: String,
}

/// One agent's notes on one set of changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notes {
    pub uid: String,
    pub agent: String,
    pub at: i64,
    pub on: On,
    #[serde(default)]
    pub summary: String,
    pub notes: Vec<Note>,
}

impl Notes {
    /// Whether these and `other` are one agent's on the same changes: the later replace the
    /// earlier.
    pub fn same_place(&self, other: &Notes) -> bool {
        self.agent == other.agent
            && match (&self.on, &other.on) {
                (On::WorkingTree, On::WorkingTree) => true,
                (On::Head { name: a, .. }, On::Head { name: b, .. }) => a == b,
                _ => false,
            }
    }
}

fn store(root: &Path) -> String {
    format!("notes/{}.json", crate::review::key(root))
}

pub fn load(root: &Path) -> Vec<Notes> {
    serde_json::from_str(&crate::storage::read(&store(root))).unwrap_or_default()
}

pub fn save(root: &Path, notes: &[Notes]) {
    if let Ok(body) = serde_json::to_string_pretty(notes) {
        crate::storage::save(&store(root), body);
    }
}

/// `taken` added to `kept`: one agent's earlier notes on the same changes go, and each new note
/// gets an id no other has.
pub fn take(kept: &mut Vec<Notes>, mut taken: Notes) {
    kept.retain(|n| !n.same_place(&taken));
    let mut next = kept
        .iter()
        .flat_map(|n| n.notes.iter().map(|x| x.id))
        .max()
        .unwrap_or(0);
    for note in &mut taken.notes {
        next += 1;
        note.id = next;
    }
    kept.push(taken);
}

/// The agents' notes in a tab: those kept for its project, and where each note on a head the diff has moved on
/// from is on the one shown, followed in the background (#46) — the entity notifies when they come.
#[derive(Default)]
pub struct AgentNotes {
    pub all: Vec<Notes>,
    /// Where a note on an earlier head is on the head shown, by its id.
    pub lines: HashMap<u64, u32>,
    task: Option<Task<()>>,
}

impl AgentNotes {
    /// Note `id`, and the notes it is one of.
    pub fn note(&self, id: u64) -> Option<(&Notes, &Note)> {
        self.all
            .iter()
            .find_map(|n| Some((n, n.notes.iter().find(|x| x.id == id)?)))
    }

    /// The notes on the changes `diff` shows, `branch` the page's: the working tree's on the working tree; a
    /// head's where the diff's head is its commit or bears its name.
    pub fn shown(&self, diff: &Diff, branch: Option<String>) -> Vec<&Notes> {
        let head = diff.header.head().map(|h| h.to_string());
        let named = match &diff.header {
            Header::Compare { head, .. } => Some(head.0.clone()),
            _ => None,
        }
        .or(branch);
        self.all
            .iter()
            .filter(|n| match (&n.on, &diff.header) {
                (On::WorkingTree, Header::WorkingTree { .. }) => true,
                (_, Header::WorkingTree { .. }) | (On::WorkingTree, _) => false,
                (On::Head { name, commit }, _) => {
                    head.as_ref() == Some(commit) || named.as_ref() == Some(name)
                }
            })
            .collect()
    }

    /// Note `id` dismissed; its notes go with their last.
    pub fn dismiss(&mut self, id: u64) {
        for n in &mut self.all {
            n.notes.retain(|x| x.id != id);
        }
        self.all
            .retain(|n| !n.notes.is_empty() || !n.summary.is_empty());
    }

    /// Dismiss all on a summary: notes `uid` and their summary go.
    pub fn dismiss_all(&mut self, uid: &str) {
        self.all.retain(|n| n.uid != uid);
    }

    /// Follows the line of every note on the diff (`diff`, with the page's `branch`) whose head it has moved on
    /// from, from the commit it was written on to the one shown, in repository `root`.
    pub fn follow(
        &mut self,
        root: Option<PathBuf>,
        diff: Option<&Diff>,
        branch: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.lines.clear();
        let (Some(root), Some(diff)) = (root, diff) else {
            return;
        };
        let Some(head) = diff.header.head() else {
            return;
        };
        let asks: Vec<(u64, String, String, u32)> = self
            .shown(diff, branch)
            .into_iter()
            .filter_map(|n| match &n.on {
                On::Head { commit, .. } if !head.to_string().starts_with(commit.as_str()) => Some(
                    n.notes
                        .iter()
                        .map(move |x| (x.id, commit.clone(), x.path.clone(), x.line)),
                ),
                _ => None,
            })
            .flatten()
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
                            match repo.follow_line(from, head, &path, line) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(agent: &str, on: On, n: usize) -> Notes {
        Notes {
            uid: format!("{agent}-{n}"),
            agent: agent.into(),
            at: 0,
            on,
            summary: "why".into(),
            notes: (0..n)
                .map(|i| Note {
                    path: "a.rs".into(),
                    line: i as u32 + 1,
                    code: "x".into(),
                    body: "b".into(),
                    ..Default::default()
                })
                .collect(),
        }
    }

    #[test]
    fn notes_taken_again_replace_the_same_agents_on_the_same_changes() {
        let head = |name: &str| On::Head {
            name: name.into(),
            commit: "c".into(),
        };
        let mut kept = Vec::new();
        take(&mut kept, notes("Claude", On::WorkingTree, 2));
        take(&mut kept, notes("Claude", head("feat"), 1));
        take(&mut kept, notes("Codex", On::WorkingTree, 1));
        assert_eq!(kept.len(), 3);
        let ids: Vec<u64> = kept
            .iter()
            .flat_map(|n| n.notes.iter().map(|x| x.id))
            .collect();
        assert_eq!(ids, [1, 2, 3, 4]);
        take(&mut kept, notes("Claude", On::WorkingTree, 1));
        assert_eq!(kept.len(), 3);
        assert_eq!(kept[2].agent, "Claude");
        assert_eq!(kept[2].notes[0].id, 5);
        let json = serde_json::to_string(&kept).unwrap();
        assert_eq!(serde_json::from_str::<Vec<Notes>>(&json).unwrap(), kept);
    }
}
