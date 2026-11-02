//! The working tree as last looked at, polled while a repository is open: its count and fingerprint, git asked when
//! the file system says something changed (#87). The poll's beat is the tab's too: what rides on it hears
//! `Event::Beat`, a change `Event::Working`.

use crate::history::{Event, History};
use artoo_core::fswatch::Changes;
use artoo_core::git::Repo;
use gpui::Context;
use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How often the working tree is looked at.
const POLL: Duration = Duration::from_secs(2);
/// The longest wait while nothing changes.
const IDLE: Duration = Duration::from_secs(8);
/// How often a tab behind another looks.
const BEHIND: Duration = Duration::from_secs(10);
/// Git is asked at least this often, whatever the file system said.
const RECHECK: Duration = Duration::from_secs(60);

/// The working tree's count and fingerprint, polled while a repository is open.
#[derive(Default)]
pub struct WorkingState {
    pub count: usize,
    pub hash: u64,
    pub only_unstaged: bool,
    pub task: Option<gpui::Task<()>>,
    /// What the file system says changed, so the poll asks git only then (#87).
    pub changes: Option<Rc<Changes>>,
}

impl History {
    /// Starts looking at the working tree of the repository open; `plain` a folder without git, where only the
    /// beat runs (the agents' answers still come, #75); `shown` whether the tab is on screen.
    pub fn start_working_poll(
        &mut self,
        plain: bool,
        shown: Rc<Cell<bool>>,
        cx: &mut Context<Self>,
    ) {
        self.wt = WorkingState {
            only_unstaged: self.wt.only_unstaged,
            ..WorkingState::default()
        };
        let changes = self
            .root
            .as_deref()
            .filter(|_| !plain)
            .map(|root| Rc::new(Changes::watch(root)));
        let task = cx.spawn({
            let changes = changes.clone();
            async move |this, cx| {
                // Unchanged polls in a row: the next waits longer, up to `IDLE` (#87).
                let mut still = 0u32;
                let mut asked = Instant::now();
                let mut first = true;
                loop {
                    // A tab behind another looks seldom; it looks again as it comes on screen.
                    if !shown.get() {
                        cx.background_executor().timer(BEHIND).await;
                        if !shown.get() {
                            continue;
                        }
                    }
                    let Ok(Some(root)) = this.read_with(cx, |h, _| h.root.clone()) else {
                        return;
                    };
                    // Git is asked when the file system says something changed, and at least once a
                    // minute (#87).
                    let changed = changes.as_ref().is_none_or(|c| c.take());
                    let due = first || changed || asked.elapsed() >= RECHECK;
                    first = false;
                    let summary = match plain {
                        true => Err(anyhow::anyhow!("not a git repository")),
                        false if !due => Err(anyhow::anyhow!("nothing changed")),
                        false => {
                            asked = Instant::now();
                            cx.background_executor()
                                .spawn(async move {
                                    Repo::open(&root).and_then(|r| r.working_summary())
                                })
                                .await
                        }
                    };
                    if let Ok((count, hash)) = summary {
                        match this.update(cx, |h, cx| h.working_summary_arrived(count, hash, cx)) {
                            Ok(true) => still = 0,
                            Ok(false) => still = still.saturating_add(1),
                            Err(_) => return,
                        }
                    }
                    // What rides on the beat looks.
                    if this.update(cx, |_, cx| cx.emit(Event::Beat)).is_err() {
                        return;
                    }
                    cx.background_executor()
                        .timer((POLL * (1 + still.min(3))).min(IDLE))
                        .await;
                }
            }
        });
        self.wt.changes = changes;
        self.wt.task = Some(task);
    }

    /// The working tree's count and fingerprint read: whether it changed, said to the tab too.
    fn working_summary_arrived(&mut self, count: usize, hash: u64, cx: &mut Context<Self>) -> bool {
        let changed = hash != self.wt.hash;
        self.wt.count = count;
        self.wt.hash = hash;
        if changed {
            cx.emit(Event::Working);
        }
        changed
    }
}
