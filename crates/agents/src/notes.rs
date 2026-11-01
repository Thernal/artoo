//! An agent's notes on the lines of its changes (#151): why this way, what it left alone, what to
//! look at closely, with a summary of the whole. An agent writes them with `annotate_changes` (or
//! `open_changes`); the app keeps them per project on this Mac, as comments for an agent are, and
//! never sends them to GitLab or GitHub. A note sits on a line of the new side, found again by the
//! line's text, and follows it from the commit it was written on (#46).

use crate::git;
use artoo_review_api::diff::{Diff, Header};
use gpui::{Context, Task};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The notes' model, the contract's.
pub use artoo_agents_api::notes::{Note, Notes, On, Shown, take};

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

/// The agents' notes in a tab: those kept for its project, and where each note on a head the diff has moved on
/// from is on the one shown, followed in the background (#46) — the entity notifies when they come.
#[derive(Default)]
pub struct AgentNotes {
    pub all: Vec<Notes>,
    /// Where a note on an earlier head is on the head shown, by its id.
    pub lines: HashMap<u64, u32>,
    /// The project they are kept for.
    root: Option<PathBuf>,
    task: Option<Task<()>>,
}

/// What `diff` is of, as the notes ask it.
pub fn shown_of(diff: &Diff) -> Shown {
    match (&diff.header, diff.header.head()) {
        (Header::WorkingTree { .. }, _) | (_, None) => Shown::WorkingTree,
        (header, Some(head)) => Shown::Head {
            commit: head.to_string(),
            name: match header {
                Header::Compare { head, .. } => Some(head.0.clone()),
                _ => None,
            },
        },
    }
}

impl AgentNotes {
    /// Project `root`'s, read anew.
    pub fn load(&mut self, root: PathBuf) {
        self.all = load(&root);
        self.root = Some(root);
    }

    /// Kept for the project, as they are now.
    pub fn save(&self) {
        if let Some(root) = &self.root {
            save(root, &self.all);
        }
    }

    /// Follows the line of every note on the changes `shown` (with the page's `branch`) whose head it has moved on
    /// from, from the commit it was written on to the one shown, in repository `root`.
    pub fn follow(
        &mut self,
        root: Option<PathBuf>,
        shown: Option<Shown>,
        branch: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.lines.clear();
        let (Some(root), Some(shown)) = (root, shown) else {
            return;
        };
        let Shown::Head {
            commit: shown_head, ..
        } = &shown
        else {
            return;
        };
        let Ok(head) = git2::Oid::from_str(shown_head) else {
            return;
        };
        let asks: Vec<(u64, String, String, u32)> =
            artoo_agents_api::notes::shown(&self.all, &shown, branch)
                .into_iter()
                .filter_map(|n| match &n.on {
                    On::Head { commit, .. } if !shown_head.starts_with(commit.as_str()) => Some(
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
