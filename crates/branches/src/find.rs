//! The commits' search (⌘⇧F): what is typed, the commits it finds among those listed, and the paths each commit
//! touched, read in the background so `path:` can be judged. The matching itself is the core's `search`.

use crate::history::History;
use artoo_core::git::Repo;
use artoo_core::search::Query;
use git2::Oid;
use gpui::{Context, Task};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Commits whose paths are read per background step.
const PATH_CHUNK: usize = 250;

#[derive(Default)]
pub struct Find {
    /// The text in the field; `None` while the field is not active.
    pub text: Option<String>,
    pub query: Query,
    /// Indices into `History::commits` of the commits that match; `None` shows every commit.
    pub shown: Option<Vec<usize>>,
    /// Paths each commit touched, read in the background after a branch loads.
    paths: HashMap<Oid, Vec<String>>,
    /// Paths are still being read.
    pub indexing: bool,
    task: Option<Task<()>>,
}

impl Find {
    /// A new repository: nothing known about its commits.
    pub fn reset(&mut self) {
        self.paths.clear();
        self.indexing = false;
        self.task = None;
        self.shown = None;
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl History {
    /// Rows in the commit list.
    pub fn commit_rows(&self) -> usize {
        self.find
            .shown
            .as_ref()
            .map_or(self.commits.len(), Vec::len)
    }

    /// The index in `commits` shown at list row `row`.
    pub fn commit_at(&self, row: usize) -> usize {
        self.find.shown.as_ref().map_or(row, |s| s[row])
    }

    /// The list row of commit `ix`, if it is shown.
    pub fn row_of(&self, ix: usize) -> Option<usize> {
        match &self.find.shown {
            None => Some(ix),
            Some(shown) => shown.iter().position(|&i| i == ix),
        }
    }

    /// Filters `commits` by the field's text; the selection stays where it is.
    pub fn refilter(&mut self) {
        let text = self.find.text.clone().unwrap_or_default();
        self.find.query = Query::parse(&text, now());
        self.find.shown = (!self.find.query.is_empty()).then(|| {
            self.commits
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    let paths = self.find.paths.get(&c.id).map(Vec::as_slice);
                    // A path condition cannot be judged before the paths are known.
                    self.find.query.matches(c, paths)
                })
                .map(|(ix, _)| ix)
                .collect()
        });
    }

    /// Reads the paths of every listed commit of the repository at `root`, a chunk at a time; the filter
    /// updates as they arrive.
    pub fn index_paths(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let ids: Vec<Oid> = self
            .commits
            .iter()
            .map(|c| c.id)
            .filter(|id| !self.find.paths.contains_key(id))
            .collect();
        if ids.is_empty() {
            self.find.indexing = false;
            return;
        }
        self.find.indexing = true;
        self.find.task = Some(cx.spawn(async move |this, cx| {
            for chunk in ids.chunks(PATH_CHUNK) {
                let chunk = chunk.to_vec();
                let root = root.clone();
                let read = cx
                    .background_executor()
                    .spawn(async move {
                        Repo::open(&root)
                            .map(|repo| repo.changed_paths(&chunk))
                            .unwrap_or_default()
                    })
                    .await;
                let alive = this.update(cx, |h, cx| {
                    h.find.paths.extend(read);
                    if h.find.query.needs_paths() || h.find.text.is_some() {
                        h.refilter();
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            this.update(cx, |h, cx| {
                h.find.indexing = false;
                cx.notify();
            })
            .ok();
        }));
    }
}
