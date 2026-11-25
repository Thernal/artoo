//! Editing a request that is there (Design round 46): a description being edited, kept on this Mac until it is
//! saved or cancelled, an agent's proposal for a request's title or description (`edit_request`), waiting for the
//! reviewer, and how a proposed text reads against the one it would replace. Nothing here reaches the host.

use crate::{storage, worddiff};
use serde::{Deserialize, Serialize};
use std::path::Path;

const FILE: &str = "request-edits.json";

/// What is kept of one request of one project.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    /// The project's folder.
    pub project: String,
    pub number: u64,
    /// The description being edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Proposal>,
}

/// An agent's proposal: a new title, a new description, or both.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When it came, seconds since the epoch.
    pub at: i64,
}

fn all() -> Vec<Kept> {
    serde_json::from_str(&storage::read(FILE)).unwrap_or_default()
}

/// What is kept of request `number` of `project`.
pub fn load(project: &Path, number: u64) -> Kept {
    let project = project.to_string_lossy().into_owned();
    (all().into_iter())
        .find(|k| k.project == project && k.number == number)
        .unwrap_or(Kept {
            project,
            number,
            ..Default::default()
        })
}

/// Keeps `kept` in place of what its request had; one with nothing in it goes.
pub fn keep(kept: Kept) {
    let list = with(all(), kept);
    if let Ok(json) = serde_json::to_string(&list) {
        storage::save(FILE, json);
    }
}

fn with(mut list: Vec<Kept>, kept: Kept) -> Vec<Kept> {
    list.retain(|k| k.project != kept.project || k.number != kept.number);
    if kept.editing.is_some() || kept.proposal.is_some() {
        list.push(kept);
    }
    list
}

/// A run of a compared text: as it was and stays, taken out, or put in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Same,
    Out,
    In,
}

/// `new` read against `old`, in order: lines both have as they are; a changed line by its words where it is still
/// the same line (`worddiff::pieces`), else taken out and put in whole.
pub fn compared(old: &str, new: &str) -> Vec<(Mark, String)> {
    let a: Vec<&str> = old.split_inclusive('\n').collect();
    let b: Vec<&str> = new.split_inclusive('\n').collect();
    let same = |i: usize, j: usize| a[i].trim_end() == b[j].trim_end();
    // lcs[i][j]: the common lines of the two ends from there.
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = match same(i, j) {
                true => lcs[i + 1][j + 1] + 1,
                false => lcs[i + 1][j].max(lcs[i][j + 1]),
            };
        }
    }
    let mut out: Vec<(Mark, String)> = Vec::new();
    let mut push = |mark: Mark, text: &str| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some((last, run)) if *last == mark => run.push_str(text),
            _ => out.push((mark, text.to_owned())),
        }
    };
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && same(i, j) {
            push(Mark::Same, b[j]);
            (i, j) = (i + 1, j + 1);
            continue;
        }
        // The lines between two common ones: paired as they come.
        let (from_i, from_j) = (i, j);
        while (i < a.len() || j < b.len()) && !(i < a.len() && j < b.len() && same(i, j)) {
            if j >= b.len() || (i < a.len() && lcs[i + 1][j] >= lcs[i][j + 1]) {
                i += 1;
            } else {
                j += 1;
            }
        }
        let (gone, came) = (&a[from_i..i], &b[from_j..j]);
        for n in 0..gone.len().max(came.len()) {
            let (old, new) = (gone.get(n), came.get(n));
            let pieces = old.zip(new).and_then(|(o, n)| worddiff::pieces(o, n));
            match (old, new, pieces) {
                (Some(o), Some(n), Some(pieces)) => {
                    for piece in pieces {
                        match piece {
                            worddiff::Piece::Same(_, y) => push(Mark::Same, &n[y]),
                            worddiff::Piece::Change(x, y) => {
                                push(Mark::Out, &o[x]);
                                push(Mark::In, &n[y]);
                            }
                        }
                    }
                }
                (old, new, _) => {
                    old.into_iter().for_each(|o| push(Mark::Out, o));
                    new.into_iter().for_each(|n| push(Mark::In, n));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_proposal_reads_against_the_text_by_its_words() {
        let old = "Moves the screen to one state. The view model keeps the error.\n\nEnd.\n";
        let new = "Moves the screen to one state. The view model keeps the error text.\n\nEnd.\nCloses PAY-398.\n";
        let marked = compared(old, new);
        let of = |mark| -> String {
            (marked.iter())
                .filter(|(m, _)| *m == mark || *m == Mark::Same)
                .map(|(_, t)| t.as_str())
                .collect()
        };
        // Either side reads back whole.
        assert_eq!(of(Mark::Out), old);
        assert_eq!(of(Mark::In), new);
        assert!(
            marked
                .iter()
                .any(|(m, t)| *m == Mark::In && t.contains("text"))
        );
        assert_eq!(compared(old, old), vec![(Mark::Same, old.to_owned())]);
        assert_eq!(
            compared("gone\n", ""),
            vec![(Mark::Out, "gone\n".to_owned())]
        );
    }

    #[test]
    fn what_has_nothing_to_keep_goes() {
        let kept = |number, editing: Option<&str>| Kept {
            project: "/p".into(),
            number,
            editing: editing.map(str::to_owned),
            proposal: None,
        };
        let list = with(vec![kept(1, Some("a"))], kept(2, Some("b")));
        assert_eq!(list.len(), 2);
        let list = with(list, kept(1, None));
        assert_eq!(list, vec![kept(2, Some("b"))]);
    }
}
