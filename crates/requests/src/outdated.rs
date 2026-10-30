//! What became of the line an outdated discussion was written on, version by version (#46), followed in the
//! background, and the comparison opened from one to step through them.

use crate::requests::{Event, Requests};
use artoo_core::git::{Fate, Repo};
use artoo_hosts_api::Thread;
use gpui::Context;
use std::collections::HashMap;
use std::path::PathBuf;

/// One later version that changed the line.
#[derive(Clone, Debug)]
pub struct Step {
    pub version: usize,
    pub who: String,
    pub summary: String,
    pub sha: String,
    /// What the line says in that version; None when it was removed.
    pub now: Option<String>,
    /// Where the line is in that version.
    pub line: Option<u32>,
}

/// What became of the line a discussion was written on.
#[derive(Clone, Debug)]
pub struct LineHistory {
    /// The version it was written on.
    pub written: usize,
    /// The line as it read then.
    pub then: String,
    pub steps: Vec<Step>,
    /// Where the line is in the latest version; None when it is gone.
    pub line_now: Option<u32>,
}

impl LineHistory {
    pub fn changed(&self) -> bool {
        !self.steps.is_empty()
    }

    /// The versions to step through: the one it was written on (at `line` then), each that
    /// changed the line, and the latest (`latest`) when the line did not change in it.
    pub fn stops(&self, line: Option<u32>, latest: usize) -> Vec<(usize, Option<u32>)> {
        let mut stops = vec![(self.written, line)];
        stops.extend(self.steps.iter().map(|s| (s.version, s.line)));
        if stops.last().is_some_and(|(v, _)| *v < latest) {
            stops.push((latest, self.line_now));
        }
        stops
    }
}

/// A comparison opened from an outdated discussion: the versions that changed its line, to step
/// through (#46 C, its stepper v3 → v5 → v7).
#[derive(Clone, Debug)]
pub struct Since {
    pub iid: u64,
    /// The discussion it was opened from.
    pub thread: String,
    pub path: String,
    /// Version numbers, oldest first, with where the line is in each (`None`: gone).
    pub stops: Vec<(usize, Option<u32>)>,
}

impl Requests {
    /// The latest version's head of the open request.
    fn latest_head(&self) -> Option<String> {
        self.current
            .and_then(|iid| self.find(iid))
            .map(|m| m.head.clone())
            .filter(|h| !h.is_empty())
    }

    /// The commit discussion `t` was written on: the host's word, else (an older GitLab answer
    /// without `head_sha`) the version pushed last before its first note.
    pub fn written_commit(&self, t: &Thread) -> Option<String> {
        if let Some(c) = &t.commit {
            return Some(c.clone());
        }
        let iid = self.current?;
        let when = t.notes.first()?.created;
        match &self.versions {
            Some((for_iid, list)) if *for_iid == iid && list.len() > 1 => list
                .iter()
                .rev()
                .find(|v| v.time <= when)
                .map(|v| v.tip.to_string()),
            _ => None,
        }
    }

    /// Written on an earlier version than the one shown: its line number is that version's.
    pub fn thread_is_old(&self, t: &Thread) -> bool {
        if t.outdated {
            return true;
        }
        match (self.written_commit(t), self.latest_head()) {
            (Some(c), Some(head)) => !head.starts_with(c.as_str()) && !c.starts_with(&head),
            _ => false,
        }
    }

    /// Follows the line of every discussion written on an earlier version to the latest, in the
    /// background, in the repository at `root`; the threads and the versions must both be in.
    /// `Event::Histories` tells the tab.
    pub fn compute_histories(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let Some(iid) = self.current else {
            return;
        };
        let versions: Vec<(usize, git2::Oid)> = match &self.versions {
            Some((for_iid, list)) if *for_iid == iid => {
                list.iter().map(|v| (v.number, v.tip)).collect()
            }
            _ => return,
        };
        let old: Vec<(String, String, String, u32)> = self
            .threads
            .iter()
            .filter(|t| self.thread_is_old(t))
            .filter_map(|t| {
                Some((
                    t.id.clone(),
                    self.written_commit(t)?,
                    t.path.clone()?,
                    t.new_line?,
                ))
            })
            .collect();
        if old.is_empty() {
            self.histories.clear();
            return;
        }
        self.history_task = Some(cx.spawn(async move |this, cx| {
            let histories = cx
                .background_executor()
                .spawn(async move {
                    let Ok(repo) = Repo::open(&root) else {
                        return HashMap::new();
                    };
                    old.into_iter()
                        .filter_map(|(id, commit, path, line)| {
                            let at = versions
                                .iter()
                                .position(|(_, tip)| tip.to_string().starts_with(&commit))?;
                            Some((id, history(&repo, &versions, at, &path, line)))
                        })
                        .collect::<HashMap<_, _>>()
                })
                .await;
            this.update(cx, |rq, cx| {
                if rq.current == Some(iid) {
                    rq.histories = histories;
                    cx.emit(Event::Histories);
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The outdated discussions by the version they were written on, newest version first.
    pub fn outdated_by_version(&self) -> Vec<(Option<usize>, Vec<usize>)> {
        let mut groups: Vec<(Option<usize>, Vec<usize>)> = Vec::new();
        for (ix, t) in self.threads.iter().enumerate() {
            if t.path.is_none() || !self.thread_is_old(t) {
                continue;
            }
            let version = self.histories.get(&t.id).map(|h| h.written);
            match groups.iter_mut().find(|(v, _)| *v == version) {
                Some((_, list)) => list.push(ix),
                None => groups.push((version, vec![ix])),
            }
        }
        groups.sort_by_key(|(v, _)| std::cmp::Reverse(*v));
        groups
    }

    /// Whether thread `t` began with a comment of yours: only then is it "your comment".
    pub fn yours(&self, t: &Thread) -> bool {
        let me = self.login.as_deref().unwrap_or_default();
        !me.is_empty() && t.notes.first().is_some_and(|n| n.login == me)
    }

    /// "Changed since your comment" (or this comment, another's), or not, for thread `ix`; None
    /// while it is not known.
    pub fn since_comment(&self, ix: usize) -> Option<String> {
        let t = self.threads.get(ix)?;
        let h = self.histories.get(&t.id)?;
        let whose = if self.yours(t) {
            "your comment"
        } else {
            "this comment"
        };
        Some(match (h.changed(), h.line_now) {
            (_, None) => format!("Line removed since {whose}"),
            (true, _) => format!("Changed since {whose}"),
            (false, _) => format!("Unchanged since {whose}"),
        })
    }
}

/// Follows `line` of `path` from version `at` to the last of `versions`.
fn history(
    repo: &Repo,
    versions: &[(usize, git2::Oid)],
    at: usize,
    path: &str,
    line: u32,
) -> LineHistory {
    let (written, tip) = versions[at];
    let then = repo
        .file_at(tip, path)
        .and_then(|t| {
            t.lines()
                .nth(line as usize - 1)
                .map(|l| l.trim_end().to_owned())
        })
        .unwrap_or_default();
    let mut steps = Vec::new();
    let mut now = Some(line);
    for pair in versions[at..].windows(2) {
        let ((_, old), (number, new)) = (pair[0], pair[1]);
        let Some(cur) = now else { break };
        let who = || {
            repo.last_change(old, new, path)
                .map(|c| (c.author, artoo_ui_kit::format::short(c.id), c.summary))
                .unwrap_or_default()
        };
        match repo.follow_line(old, new, path, cur) {
            Fate::Same(n) => now = Some(n),
            Fate::Changed(n, text) => {
                let (who, sha, summary) = who();
                steps.push(Step {
                    version: number,
                    who,
                    summary,
                    sha,
                    now: Some(text),
                    line: Some(n),
                });
                now = Some(n);
            }
            Fate::Gone => {
                let (who, sha, summary) = who();
                steps.push(Step {
                    version: number,
                    who,
                    summary,
                    sha,
                    now: None,
                    line: None,
                });
                now = None;
            }
        }
    }
    LineHistory {
        written,
        then,
        steps,
        line_now: now,
    }
}
