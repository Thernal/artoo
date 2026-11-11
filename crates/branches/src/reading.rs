//! What the history reads from the repository, each read its own task: a branch — its versions, what its page
//! says, its commits —, and older pages of its history. What the tab does after one comes to it as an `Event`.

use crate::facts::Facts;
use crate::history::{Event, History, NamedTip, Older, RequestSides};
use artoo_core::git::{CommitInfo, Repo, Version};
use git2::Oid;
use gpui::Context;

/// How much of a branch's history is read at first; more is read as the list is scrolled (#25).
pub const COMMIT_PAGE: usize = 1000;

/// A branch as read, before the tab shows it.
pub struct Read {
    versions: Vec<Version>,
    log: Vec<CommitInfo>,
    facts: Facts,
    /// Its own commits, when it forks from the main line.
    own: Option<Vec<CommitInfo>>,
    tip: Oid,
    keep: Option<Oid>,
}

/// How the branch read is shown.
pub struct Shown {
    /// Its page is the branch against the main line, with its own commits (#74).
    pub tabbed: bool,
    /// The commit asked for.
    pub keep: Option<Oid>,
}

impl History {
    /// Reads branch `ix` down to commit `keep` however old (#25): its versions, what its page says, and its own
    /// commits when it forks from the main line (#74).
    pub fn read_branch(&mut self, ix: usize, keep: Option<Oid>, cx: &mut Context<Self>) {
        let (Some(root), Some(branch)) = (self.root.clone(), self.branches.get(ix)) else {
            return;
        };
        let (refname, name, tip) = (branch.refname.clone(), branch.name.clone(), branch.tip);
        self.branch = Some(ix);
        self.task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let repo = Repo::open(&root)?;
                    // A ref without a reflog simply has no earlier versions.
                    let versions = repo.versions(&refname).unwrap_or_default();
                    let facts = Facts::read(&repo, &refname, &name, tip);
                    // A branch off the main line lists its own commits (#74).
                    let own = facts
                        .base
                        .as_ref()
                        .and_then(|(_, base)| repo.range_log(*base, tip, true).ok())
                        .filter(|own| !own.is_empty());
                    // Down to the commit asked for, however old (#25).
                    let log = repo.log(tip, 0, COMMIT_PAGE, keep)?;
                    anyhow::Ok(Read {
                        versions,
                        log,
                        facts,
                        own,
                        tip,
                        keep,
                    })
                })
                .await;
            this.update(cx, |h, cx| {
                // Read: the commits list stops shimmering.
                h.task = None;
                match read {
                    Ok(read) => {
                        h.read = Some(read);
                        cx.emit(Event::Read);
                    }
                    Err(err) => cx.emit(Event::Failed(format!("{err:#}"))),
                }
            })
            .ok();
        }));
    }

    /// Shows the branch read: its page shows the branch against the main line — its own commits — unless a
    /// commit of it (not one of its own) or a request on it (`pending_request`) is what was asked for; else a page
    /// of its history. None when nothing was read.
    pub fn settle(&mut self) -> Option<Shown> {
        let request = self.pending_request.is_some();
        let Read {
            versions,
            log,
            facts,
            own,
            tip,
            keep,
        } = self.read.take()?;
        self.versions = versions;
        let tabbed = own.is_some()
            && !request
            && keep.is_none_or(|id| own.as_ref().is_some_and(|o| o.iter().any(|c| c.id == id)));
        self.commits = match own {
            Some(own) if tabbed => own,
            _ => {
                let more = log.len() >= COMMIT_PAGE;
                self.older = more.then_some(Older { tip, task: None });
                log
            }
        };
        self.page = Some(Facts {
            base: facts.base.filter(|_| tabbed),
            ..facts
        });
        Some(Shown { tabbed, keep })
    }

    /// The request that waited for its branch (#71), open now: its commits listed, the host's versions of it in
    /// the timeline (`versions`, as `show_request_versions` takes them). Its sides, when one waited.
    pub fn open_pending_request(
        &mut self,
        versions: Option<&(u64, Vec<Version>)>,
    ) -> Option<RequestSides> {
        let sides = self.pending_request.take()?;
        self.open_request = Some(sides.clone());
        // Opened anew: the version you read is taken again (#187).
        self.kept_read = None;
        self.show_request_commits(&sides.0, &sides.1);
        self.show_request_versions(versions);
        Some(sides)
    }

    /// The commits of a request open: only what `head` adds to `base`, its target.
    pub fn show_request_commits(&mut self, base: &NamedTip, head: &NamedTip) {
        let Some(root) = self.root.as_ref() else {
            return;
        };
        if let Ok(list) = Repo::open(root).and_then(|r| r.range_log(base.1, head.1, true)) {
            self.commits = list;
            self.older = None;
            self.refilter();
        }
    }

    /// The host's versions of the request open in the timeline, when it has more than one: `versions` are the
    /// ones read last, with the number of the request they are of.
    pub fn show_request_versions(&mut self, versions: Option<&(u64, Vec<Version>)>) {
        let Some(iid) = self.open_request.as_ref().map(|r| r.2) else {
            return;
        };
        if let Some((_, list)) = versions.filter(|(of, list)| *of == iid && list.len() > 1) {
            self.versions = list.clone();
        }
    }

    /// The branch's older history, a page more (#25), unless one is on its way: a page is as long as what is
    /// listed, so a search through a long history reads it in a few.
    pub fn read_older(&mut self, cx: &mut Context<Self>) {
        let (Some(root), Some(older)) = (
            self.root.clone(),
            self.older.as_ref().filter(|o| o.task.is_none()),
        ) else {
            return;
        };
        let (tip, skip) = (older.tip, self.commits.len());
        let count = skip.max(COMMIT_PAGE);
        let task = cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { Repo::open(&root)?.log(tip, skip, count, None) })
                .await;
            this.update(cx, |h, cx| {
                // Another branch, or the history read again meanwhile: not these.
                if h.older.as_ref().is_none_or(|o| o.tip != tip) || h.commits.len() != skip {
                    return;
                }
                match read {
                    Ok(page) => {
                        let more = page.len() >= count;
                        h.commits.extend(page);
                        h.older = more.then_some(Older { tip, task: None });
                        cx.emit(Event::Older);
                    }
                    Err(err) => {
                        h.older = None;
                        cx.emit(Event::Failed(format!("{err:#}")));
                    }
                }
            })
            .ok();
        });
        if let Some(older) = self.older.as_mut() {
            older.task = Some(task);
        }
    }
}
