//! A tab's comments to agents (#10, #46, #76): the reviewer's line comments of this repository, kept on this Mac
//! (`review::load`, `review::save`), and what the agents answered on them, taken in on the poll's beat. What a
//! change of them means on screen — the lines' pins, a toast — is the tab's.

use crate::agents::{self, Bus};
use crate::review::{self, Comment, State};
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
}
