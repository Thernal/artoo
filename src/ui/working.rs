//! The working tree against `HEAD`: a pinned entry above the commits, kept current while an agent
//! works. Read-only — Artoo never stages or commits. Its chip on the Commits label is the branches'
//! (`artoo_branches::side::working_chip`).

use super::{Selection, WorkingState, Workspace};
use crate::git::Repo;
use gpui::Context;
use std::time::Duration;

/// How often the working tree is looked at.
const POLL: Duration = Duration::from_secs(2);
/// The longest wait while nothing changes.
const IDLE: Duration = Duration::from_secs(8);
/// How often a tab behind another looks.
const BEHIND: Duration = Duration::from_secs(10);
/// Git is asked at least this often, whatever the file system said.
const RECHECK: Duration = Duration::from_secs(60);

impl Workspace {
    /// Starts looking at the working tree of the open repository.
    pub(super) fn start_working_poll(&mut self, cx: &mut Context<Self>) {
        self.history.update(cx, |h, _| {
            h.wt = WorkingState {
                only_unstaged: h.wt.only_unstaged,
                ..WorkingState::default()
            }
        });
        let shown = self.on_screen.clone();
        let changes = self
            .root
            .as_deref()
            .filter(|_| !self.plain)
            .map(|root| std::rc::Rc::new(crate::fswatch::Changes::watch(root)));
        let task = cx.spawn({
            let changes = changes.clone();
            async move |this, cx| {
                // Unchanged polls in a row: the next waits longer, up to `IDLE` (#87).
                let mut still = 0u32;
                let mut asked = std::time::Instant::now();
                let mut first = true;
                loop {
                    // A tab behind another looks seldom; it looks again as it comes on screen.
                    if !shown.get() {
                        cx.background_executor().timer(BEHIND).await;
                        if !shown.get() {
                            continue;
                        }
                    }
                    let Ok(Some((root, plain))) =
                        this.read_with(cx, |this, _| this.root.clone().map(|r| (r, this.plain)))
                    else {
                        return;
                    };
                    // Git is asked when the file system says something changed, and at least once a
                    // minute (#87).
                    let changed = changes.as_ref().is_none_or(|c| c.take());
                    let due = first || changed || asked.elapsed() >= RECHECK;
                    first = false;
                    // A plain folder has no git to ask; the agents' answers still come (#75).
                    let summary = match plain {
                        true => Err(anyhow::anyhow!("not a git repository")),
                        false if !due => Err(anyhow::anyhow!("nothing changed")),
                        false => {
                            asked = std::time::Instant::now();
                            cx.background_executor()
                                .spawn(async move {
                                    Repo::open(&root).and_then(|r| r.working_summary())
                                })
                                .await
                        }
                    };
                    if let Ok((count, hash)) = summary {
                        match this.update(cx, |this, cx| {
                            let changed = hash != this.history.read(cx).wt.hash;
                            this.working_summary_arrived(count, hash, cx);
                            changed
                        }) {
                            Ok(true) => still = 0,
                            Ok(false) => still = still.saturating_add(1),
                            Err(_) => return,
                        }
                    }
                    // The agents' answers ride on the same beat, and pictures that came in.
                    if this
                        .update(cx, |this, cx| {
                            this.poll_agents(cx);
                            if crate::pictures::take_fresh() {
                                cx.notify();
                            }
                        })
                        .is_err()
                    {
                        return;
                    }
                    cx.background_executor()
                        .timer((POLL * (1 + still.min(3))).min(IDLE))
                        .await;
                }
            }
        });
        self.history.update(cx, |h, _| {
            h.wt.changes = changes;
            h.wt.task = Some(task);
        });
    }

    fn working_summary_arrived(&mut self, count: usize, hash: u64, cx: &mut Context<Self>) {
        let changed = hash != self.history.read(cx).wt.hash;
        self.history.update(cx, |h, _| {
            h.wt.count = count;
            h.wt.hash = hash;
        });
        if !changed {
            return;
        }
        self.review.update(cx, |rv, _| rv.blames.working_changed());
        if self.nav.space == super::spaces::Space::Branches && self.at_root_list() {
            self.read_tree_files(cx);
        }
        // Code's tree of the working tree follows it (#75).
        let working = self.code().is_some_and(|c| c.revision(cx).is_none());
        if self.nav.space == super::spaces::Space::Code && working {
            self.read_code_files(true, cx);
        }
        if self.history.read(cx).selection == Selection::WorkingTree {
            // Reload in place: the selected file stays.
            let keep = self.review.read(cx).current_path();
            self.load_working_tree(keep, cx);
        }
        cx.notify();
    }

    pub(super) fn select_working_tree(&mut self, cx: &mut Context<Self>) {
        self.arrive(super::nav::Page::WorkingTree, cx);
        self.load_working_tree(None, cx);
    }

    pub(super) fn load_working_tree(&mut self, keep: Option<String>, cx: &mut Context<Self>) {
        if self.root.is_none() {
            return;
        }
        let only_unstaged = self.history.read(cx).wt.only_unstaged;
        // A reload finds a file partly staged at its place, in the group it was in (#22).
        let review = self.review_handle();
        self.history.update(cx, |h, cx| {
            h.show_working_tree(only_unstaged, keep, &*review, cx)
        });
        cx.notify();
    }

    pub(super) fn set_only_unstaged(&mut self, on: bool, cx: &mut Context<Self>) {
        self.history.update(cx, |h, _| h.wt.only_unstaged = on);
        let keep = self.review.read(cx).current_path();
        self.load_working_tree(keep, cx);
    }
}
