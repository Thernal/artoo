//! An agent's notes on the lines of its changes (#151; #165: the agents' model): why this way, what it left alone,
//! what to look at closely, with a summary of the whole. An agent writes them with `annotate_changes` (or
//! `open_changes`); the agents keep them per project on this Mac, as comments for an agent are, and never send them
//! to GitLab or GitHub. A note sits on a line of the new side, found again by the line's text, and follows it from
//! the commit it was written on (#46).

use serde::{Deserialize, Serialize};

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

/// What the diff shown is of, as the notes ask it: the working tree, or a head by its commit and the name it was
/// compared by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shown {
    WorkingTree,
    Head {
        commit: String,
        name: Option<String>,
    },
}

/// The notes of `all` on the changes `shown`, `branch` the page's: the working tree's on the working tree; a head's
/// where the diff's head is its commit or bears its name.
pub fn shown<'a>(all: &'a [Notes], shown: &Shown, branch: Option<String>) -> Vec<&'a Notes> {
    all.iter()
        .filter(|n| match (&n.on, shown) {
            (On::WorkingTree, Shown::WorkingTree) => true,
            (_, Shown::WorkingTree) | (On::WorkingTree, _) => false,
            (
                On::Head { name, commit },
                Shown::Head {
                    commit: head,
                    name: named,
                },
            ) => head == commit || named.as_ref().or(branch.as_ref()) == Some(name),
        })
        .collect()
}

/// Note `id` of `all`, and the notes it is one of.
pub fn note(all: &[Notes], id: u64) -> Option<(&Notes, &Note)> {
    all.iter()
        .find_map(|n| Some((n, n.notes.iter().find(|x| x.id == id)?)))
}

/// Note `id` dismissed; its notes go with their last.
pub fn dismiss(all: &mut Vec<Notes>, id: u64) {
    for n in all.iter_mut() {
        n.notes.retain(|x| x.id != id);
    }
    all.retain(|n| !n.notes.is_empty() || !n.summary.is_empty());
}

/// Dismiss all on a summary: notes `uid` and their summary go.
pub fn dismiss_all(all: &mut Vec<Notes>, uid: &str) {
    all.retain(|n| n.uid != uid);
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

    #[test]
    fn a_note_dismissed_takes_its_notes_with_its_last_unless_a_summary_stays() {
        let mut kept = Vec::new();
        take(&mut kept, notes("Claude", On::WorkingTree, 2));
        take(&mut kept, notes("Codex", On::WorkingTree, 1));
        kept[1].summary.clear();
        dismiss(&mut kept, 3);
        assert_eq!(
            kept.len(),
            1,
            "Codex's last note gone, and no summary of it"
        );
        dismiss(&mut kept, 1);
        dismiss(&mut kept, 2);
        assert_eq!(kept.len(), 1, "Claude's summary stays");
        dismiss_all(&mut kept, "Claude-2");
        assert!(kept.is_empty());
    }
}
