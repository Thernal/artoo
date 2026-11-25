//! What the requests read from their host and keep (#165 R5c): the open list, the requests gone from it, the
//! merged and closed pages, the request opened and its discussions again after a change, its versions, the
//! reactions on its notes, where merging it stands. Each read is the entity's own task; what the tab does after one comes to it
//! as an `Event`.

use crate::requests::{Event, HostSides, Requests, Scope};
use crate::waiting::Base;
use artoo_core::git::Repo;
use artoo_core::mr::Mr;
use artoo_hosts_api::{self as mr, Host};
use gpui::Context;
use std::path::PathBuf;
use std::sync::Arc;

/// How many pages' state `forget_open` keeps.
const KEPT_PAGES: usize = 8;

/// What a page showed when it was left: shown again at once on a reopen.
pub struct KeptPage {
    base: Base,
    from: u64,
    pipeline: Option<mr::Pipeline>,
    details: Option<mr::Details>,
    merge: Option<mr::MergeState>,
    read_at: Option<i64>,
    side_in: bool,
    scroll: gpui::Point<gpui::Pixels>,
}

/// What the page's side reads brought, read in parallel.
struct Side {
    drafts: Option<Vec<mr::Draft>>,
    pipeline: Option<mr::Pipeline>,
    approvals: Option<mr::Approvals>,
    details: Option<mr::Details>,
    merge: Option<mr::MergeState>,
    login: Option<String>,
}

impl Requests {
    /// Shows the open list the last run kept for the project, until the host answers: only with a token, and only
    /// when nothing is listed yet. It carries the time it was read (`listed_at`, "From HH:MM").
    pub(crate) fn load_kept_list(&mut self) {
        if !self.available || !self.list.is_empty() || self.listed_at.is_some() {
            return;
        }
        let Some(key) = self.rows.kept_key() else {
            return;
        };
        if let Some((at, list)) = crate::disk::read::<Mr>(crate::disk::LIST_FILE, key) {
            self.list = list;
            self.listed_at = Some(at);
            self.from_disk = true;
        }
    }

    /// Reads the open requests of the repository's project and tells the tab (`Event::Listed`) as soon as they are
    /// in (with who the token is when not yet known, read beside it: the menu counts the requests asking for them);
    /// then, in parallel, the project's members and who could not be reached. The hosts keep those for 15 minutes, so
    /// a warm read is the list alone.
    pub fn read_list(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        self.fresh.begin(
            self.list.is_empty() && self.listed_at.is_none(),
            std::time::Instant::now(),
        );
        self.loading = true;
        let known = self.login.is_some();
        let facts_host = host.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let side = host.clone();
            let (result, login) = cx
                .background_executor()
                .spawn(async move {
                    std::thread::scope(|s| {
                        let login = s.spawn(|| if known { None } else { host.username().ok() });
                        let list = host.list();
                        (list, login.join().ok().flatten())
                    })
                })
                .await;
            let listed = this
                .update(cx, |rq, cx| {
                    rq.loading = false;
                    rq.fresh.end(std::time::Instant::now());
                    let ok = match result {
                        Ok(list) => {
                            let at = artoo_ui_kit::format::now();
                            rq.keep_list(at, &list, cx);
                            let left = rq.current.filter(|iid| {
                                !list.iter().any(|m| m.iid == *iid)
                                    && !rq.merged_here.contains(iid)
                                    && rq.page.merged_by.is_none()
                                    && rq.list.iter().any(|m| {
                                        m.iid == *iid && m.state == artoo_core::mr::MrState::Opened
                                    })
                            });
                            rq.take_read(list);
                            if let Some(iid) = left {
                                rq.look_for_merge(iid, facts_host.clone(), cx);
                            }
                            rq.listed_at = Some(at);
                            rq.from_disk = false;
                            rq.error = None;
                            rq.refused = false;
                            if login.is_some() {
                                rq.login = login;
                            }
                            // Every row's pipeline, approvals, discussions and size (#189).
                            rq.read_facts(facts_host, cx);
                            true
                        }
                        Err(e) => {
                            let said = format!("{e:#}");
                            rq.refused = said.contains("token");
                            rq.error = Some(said);
                            false
                        }
                    };
                    cx.emit(Event::Listed);
                    cx.notify();
                    ok
                })
                .unwrap_or(false);
            if !listed {
                return;
            }
            let (members, unreached) = cx
                .background_executor()
                .spawn(async move {
                    std::thread::scope(|s| {
                        let members = s.spawn(|| side.members().unwrap_or_default());
                        let unreached = side.unreached().unwrap_or_default();
                        (members.join().unwrap_or_default(), unreached)
                    })
                })
                .await;
            this.update(cx, |rq, cx| {
                if !members.is_empty() {
                    rq.members = members;
                }
                rq.unreached = unreached;
                cx.notify();
            })
            .ok();
        }));
    }

    /// A read landed: the rows' fields change in place; a request that came in, moved to another group or left
    /// the list is held (`held`) with the rows as they were, until `apply_held`.
    fn take_read(&mut self, read: Vec<Mr>) {
        let merged = crate::held::merge(
            &self.list,
            read,
            |m| self.group_of(m),
            &self.merged_here,
            self.rows.picked,
        );
        self.list = merged.shown;
        self.held = merged.held;
    }

    /// Shows what is held: the list becomes what the host gave, the facts of the requests that came in are read.
    pub fn apply_held(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let Some(held) = self.held.take() else {
            return;
        };
        // The request on screen stays readable when it leaves the list.
        if let Some(iid) = self.current
            && !held.list.iter().any(|m| m.iid == iid)
            && !self.left.iter().any(|m| m.iid == iid)
            && let Some(m) = self.list.iter().find(|m| m.iid == iid)
        {
            self.left.push(m.clone());
        }
        self.list = held.list;
        self.merged_here.retain(|iid| Some(*iid) == self.current);
        self.read_facts(host, cx);
        cx.notify();
    }

    /// What the list's freshness slot says, from what exists: a read in flight, the last error, the kept copy, what
    /// is held.
    pub fn slot(&self) -> artoo_ui_kit::fresh::Slot {
        use artoo_ui_kit::{format::clock, fresh::Slot};
        let at = self.listed_at.map(clock);
        if self.loading {
            return if self.list.is_empty() && at.is_none() {
                Slot::Reading
            } else {
                Slot::Refreshing
            };
        }
        if self.error.is_some() {
            return match (self.refused, at) {
                (true, _) => Slot::TokenExpired,
                (false, Some(at)) => Slot::CantReach(at),
                (false, None) => Slot::CantRead,
            };
        }
        let Some(at) = at else {
            return Slot::Reading;
        };
        if self.from_disk {
            return Slot::Kept(at);
        }
        match self.held.as_ref().filter(|h| h.count() > 0) {
            Some(held) => Slot::Waiting(at, held.count(), held.kind()),
            None => Slot::Updated(at),
        }
    }

    /// Keeps the open list on disk for the next start, written from the background executor.
    fn keep_list(&self, at: i64, list: &[Mr], cx: &mut Context<Self>) {
        let Some(key) = self.rows.kept_key().map(str::to_owned) else {
            return;
        };
        let open: Vec<Mr> = (list.iter())
            .filter(|m| m.state == artoo_core::mr::MrState::Opened)
            .cloned()
            .collect();
        cx.background_executor()
            .spawn(async move { crate::disk::write(crate::disk::LIST_FILE, &key, at, &open) })
            .detach();
    }

    /// What became of requests `iids`, open in the menu but gone from the open list, or a link's
    /// that is not open: each read once, kept in `left`. `Event::Left` tells the tab.
    pub fn read_left(&mut self, iids: Vec<u64>, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let iids: Vec<u64> = iids
            .into_iter()
            .filter(|iid| self.tried.insert(*iid))
            .collect();
        if iids.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    iids.into_iter()
                        .filter_map(|iid| host.one(iid).ok())
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |rq, cx| {
                for m in read {
                    if rq.find(m.iid).is_none() {
                        rq.left.push(m);
                    }
                }
                cx.emit(Event::Left);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The next page of `scope`'s merged or closed requests, unless one is on its way or none
    /// follows.
    pub fn load_past_page(&mut self, scope: Scope, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let state = self.past.of;
        let whose = match (scope, self.login.clone()) {
            (Scope::All, _) => mr::Whose::All,
            (Scope::ReviewMe, Some(login)) => mr::Whose::Reviewer(login),
            (Scope::Mine, Some(login)) => mr::Whose::Author(login),
            // Who the token is not known yet: read once it is, as the list is drawn.
            _ => return,
        };
        let pages = self.past.of_scope_mut(scope);
        if pages.loading || !pages.more || state == mr::MrState::Opened {
            return;
        }
        pages.loading = true;
        let at = pages.at.clone();
        let task = cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { host.list_past(state, &whose, at.as_deref()) })
                .await;
            this.update(cx, |rq, cx| {
                if rq.past.of != state {
                    return;
                }
                let pages = rq.past.of_scope_mut(scope);
                pages.loading = false;
                match read {
                    Ok(page) => {
                        pages.more = page.next.is_some();
                        pages.at = page.next;
                        pages.total = page.total.or(pages.total);
                        pages.error = None;
                        let known: std::collections::HashSet<u64> =
                            pages.list.iter().map(|m| m.iid).collect();
                        pages
                            .list
                            .extend(page.list.into_iter().filter(|m| !known.contains(&m.iid)));
                    }
                    Err(e) => {
                        pages.more = false;
                        pages.error = Some(format!("{e:#}"));
                    }
                }
                cx.notify();
            })
            .ok();
        });
        self.past.of_scope_mut(scope).task = Some(task);
    }

    /// The page of the scope shown that could not be read, again.
    pub fn retry_past_page(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let scope = self.scope;
        let pages = self.past.of_scope_mut(scope);
        pages.error = None;
        pages.more = true;
        self.load_past_page(scope, host, cx);
        cx.notify();
    }

    /// Reads request `iid`, just opened: its discussions first (`Event::Opened` tells the tab as soon as they are in),
    /// beside them, in parallel, its pending comments, pipeline, approvals and details, where merging it stands while
    /// it is open, and who the token is once (`Event::Read` tells the tab when those are in). What the last pages of
    /// the same request showed, or the row's facts, are shown at once. A cancel flag (`rq.current` moving on) is
    /// checked before the rest of the calls start.
    pub fn read_open(&mut self, iid: u64, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let known = self.login.is_some();
        let open = self
            .find(iid)
            .is_some_and(|m| m.state == mr::MrState::Opened);
        // A page already shown is read again in place; one just opened shows what is kept of it first.
        if !self.page_shown(iid) {
            if self.page.of != Some(iid) {
                self.page_opened(iid);
            }
            self.show_kept(iid, cx);
        }
        self.page_reads();
        self.reading = true;
        let from = self.waiting.tick();
        let side_host = host.clone();
        self.thread_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { host.threads(iid) })
                .await;
            this.update(cx, |rq, cx| {
                if rq.current != Some(iid) {
                    return;
                }
                rq.reading = false;
                let read = match result {
                    Ok(threads) => {
                        let base = Base {
                            threads,
                            drafts: rq.base.drafts.clone(),
                            approvals: rq.base.approvals.clone(),
                        };
                        let before = rq.base.threads.clone();
                        rq.take_base(from, base);
                        rq.page_read(&before);
                        true
                    }
                    Err(e) => {
                        rq.page_failed(&format!("{e:#}"));
                        false
                    }
                };
                cx.emit(Event::Opened { iid, read });
                cx.notify();
            })
            .ok();
        }));
        self.side_task = Some(cx.spawn(async move |this, cx| {
            let host = side_host;
            let still = this.clone();
            let cancelled = move |cx: &gpui::AsyncApp| {
                still
                    .read_with(cx, |rq, _| rq.current != Some(iid))
                    .unwrap_or(true)
            };
            if cancelled(cx) {
                return;
            }
            let read = cx
                .background_executor()
                .spawn(async move {
                    std::thread::scope(|s| {
                        // Drafts need a recent GitLab; without them the rest still works.
                        let drafts = s.spawn(|| host.drafts(iid).ok());
                        let pipeline = s.spawn(|| host.pipeline(iid).ok().flatten());
                        let approvals = s.spawn(|| host.approvals(iid).ok());
                        let details = s.spawn(|| host.details(iid).ok());
                        let merge = s.spawn(|| open.then(|| host.merge_state(iid).ok()).flatten());
                        let login = s.spawn(|| if known { None } else { host.username().ok() });
                        Side {
                            drafts: drafts.join().ok().flatten(),
                            pipeline: pipeline.join().ok().flatten(),
                            approvals: approvals.join().ok().flatten(),
                            details: details.join().ok().flatten(),
                            merge: merge.join().ok().flatten(),
                            login: login.join().ok().flatten(),
                        }
                    })
                })
                .await;
            this.update(cx, |rq, cx| {
                if rq.current != Some(iid) {
                    return;
                }
                if read.login.is_some() {
                    rq.login = read.login;
                }
                // A part that did not answer keeps what shows.
                rq.details = read.details.or_else(|| rq.details.take());
                rq.merge = read.merge.or_else(|| rq.merge.take().filter(|_| open));
                rq.pipeline = read.pipeline.or_else(|| rq.pipeline.take());
                rq.page.side_in = true;
                if from >= rq.from {
                    if let Some(drafts) = read.drafts {
                        rq.base.drafts = drafts;
                    }
                    if read.approvals.is_some() {
                        rq.base.approvals = read.approvals;
                    }
                }
                cx.emit(Event::Read(iid));
                cx.notify();
            })
            .ok();
        }));
    }

    /// Shows at once what is kept of request `iid` — its page as the last visit left it, else its row's facts —
    /// until the host answers.
    fn show_kept(&mut self, iid: u64, cx: &mut Context<Self>) {
        if self.restore(iid) {
            cx.emit(Event::Read(iid));
        }
    }

    /// Puts the kept state of request `iid` back; whether there was any.
    pub(crate) fn restore(&mut self, iid: u64) -> bool {
        if let Some(ix) = self.pages.iter().position(|(n, _)| *n == iid) {
            let (_, kept) = self.pages.remove(ix);
            self.base = kept.base;
            self.from = kept.from;
            self.pipeline = kept.pipeline;
            self.details = kept.details;
            self.merge = kept.merge;
            self.page.of = Some(iid);
            self.page.read_at = kept.read_at;
            self.page.side_in = kept.side_in;
            self.page.kept = kept.read_at.is_some();
            self.page.scroll.set_offset(kept.scroll);
        } else if let Some(f) = self.rows.facts(iid) {
            self.base = Base {
                threads: f.threads.clone(),
                drafts: Vec::new(),
                approvals: f.approvals.clone(),
            };
            self.pipeline = f.pipeline.clone();
        } else {
            return false;
        }
        true
    }

    /// What was read of the request open is let go: another one comes, or none. The last 8 pages' state is kept, to
    /// show at once on a reopen. A merge's card is of the request it was opened on (#164).
    pub fn forget_open(&mut self) {
        if self.merging.as_ref().is_some_and(|c| !c.busy) {
            self.merging = None;
        }
        if let Some(iid) = self.current
            && (!self.base.threads.is_empty() || self.page_shown(iid))
        {
            self.pages.retain(|(n, _)| *n != iid);
            self.pages.push((
                iid,
                KeptPage {
                    base: std::mem::take(&mut self.base),
                    from: self.from,
                    pipeline: self.pipeline.take(),
                    details: self.details.take(),
                    merge: self.merge.take(),
                    read_at: self.page.read_at.filter(|_| self.page.of == Some(iid)),
                    side_in: self.page.side_in,
                    scroll: self.page.scroll.offset(),
                },
            ));
            if self.pages.len() > KEPT_PAGES {
                self.pages.remove(0);
            }
        }
        self.thread_task = None;
        self.side_task = None;
        self.threads.clear();
        self.drafts.clear();
        self.base = Default::default();
        self.reload_task = None;
        self.pipeline = None;
        self.approvals = None;
        self.merge = None;
        self.merge_busy = None;
        self.details = None;
        self.histories.clear();
        self.awards.clear();
        self.page.of = None;
        self.page.read_at = None;
        self.page.merged_by = None;
        self.page.new_notes.clear();
    }

    /// Reads the discussions, pending comments and approvals of the open request again (after a
    /// change to it) and swaps them in, nothing cleared first; a read that fails leaves what
    /// shows (#139). `Event::Read` tells the tab.
    pub fn reload_threads(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let Some(iid) = self.current else {
            return;
        };
        let from = self.waiting.tick();
        self.page_reads();
        self.reload_task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    let drafts = host.drafts(iid).ok();
                    let approvals = host.approvals(iid).ok();
                    host.threads(iid).map(|t| (t, drafts, approvals))
                })
                .await;
            this.update(cx, |rq, cx| {
                if rq.current != Some(iid) {
                    return;
                }
                match read {
                    Ok((threads, drafts, approvals)) => {
                        let base = Base {
                            threads,
                            drafts: drafts.unwrap_or_else(|| rq.base.drafts.clone()),
                            approvals: approvals.or_else(|| rq.base.approvals.clone()),
                        };
                        let before = rq.base.threads.clone();
                        let taken = rq.take_base(from, base);
                        rq.page_read(&before);
                        if taken {
                            cx.emit(Event::Read(iid));
                        }
                    }
                    Err(e) => rq.page_failed(&format!("{e:#}")),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// The host's answer for the open request, from a read begun at tick `from`.
    /// A read begun at an older tick than the one taken already is dropped: its answer predates a change the other
    /// one holds. Whether it was taken.
    pub fn take_base(&mut self, from: u64, base: Base) -> bool {
        if from < self.from {
            return false;
        }
        self.base = base;
        self.from = from;
        true
    }

    /// Reads the pushes the host kept for request `iid` and makes them versions in the repository
    /// at `root`: the versions timeline once the request's diff is open, the latest one's base and
    /// head what it is compared from (#145). `Event::Versions` tells the tab.
    pub fn load_versions(
        &mut self,
        iid: u64,
        host: Arc<dyn Host>,
        root: PathBuf,
        cx: &mut Context<Self>,
    ) {
        self.version_task = Some(cx.spawn(async move |this, cx| {
            let built = cx
                .background_executor()
                .spawn(async move {
                    let read = host.versions(iid)?;
                    let oid = |sha: &str| git2::Oid::from_str(sha).ok();
                    let host: Option<HostSides> = read.last().map(|p| (oid(&p.base), oid(&p.head)));
                    let pushes: Vec<_> = read
                        .iter()
                        .filter_map(|p| Some((oid(&p.head)?, oid(&p.base), p.created)))
                        .collect();
                    let ids: Vec<_> = read
                        .iter()
                        .filter_map(|p| Some((oid(&p.head)?, p.id)))
                        .collect();
                    let list = Repo::open(&root)?.versions_from_pushes(&pushes);
                    anyhow::Ok((list, host, ids))
                })
                .await;
            this.update(cx, |rq, cx| {
                if let Ok((list, host, ids)) = built {
                    rq.versions = Some((iid, list));
                    rq.version_ids = Some((iid, ids));
                    rq.host = host.map(|h| (iid, h));
                    cx.emit(Event::Versions(iid));
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Reads the reactions on every note of the open request, in the background. `Event::Awards`
    /// tells the tab.
    pub fn load_awards(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let Some(iid) = self.current else {
            return;
        };
        let notes: Vec<(u64, bool)> = self
            .threads
            .iter()
            .flat_map(|t| {
                let issue = t.id.starts_with("issue|");
                t.notes.iter().map(move |n| (n.id, issue))
            })
            .collect();
        self.award_task = Some(cx.spawn(async move |this, cx| {
            let awards = cx
                .background_executor()
                .spawn(async move { host.awards(iid, &notes) })
                .await;
            this.update(cx, |rq, cx| {
                if rq.current == Some(iid)
                    && let Ok(mut awards) = awards
                {
                    // One's own still on its way stays (#139).
                    for (note, list) in &rq.awards {
                        for a in list.iter().filter(|a| a.id == 0) {
                            let fresh = awards.entry(*note).or_default();
                            if !fresh.iter().any(|f| f.name == a.name && f.user == a.user) {
                                fresh.push(a.clone());
                            }
                        }
                    }
                    rq.awards = awards;
                    cx.emit(Event::Awards);
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Reads where merging the open request stands again: after a rebase, or auto-merge set or
    /// cancelled.
    pub fn reload_merge(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let Some(iid) = self.current else {
            return;
        };
        self.merge_task = Some(cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { host.merge_state(iid) })
                .await;
            this.update(cx, |rq, cx| {
                if rq.current == Some(iid) {
                    rq.merge_busy = None;
                    if let Ok(state) = read {
                        rq.merge = Some(state);
                    }
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Rebases the open request on GitLab (#244), without a pipeline when `skip_ci` (#245, the project's way from now
    /// on): GitLab answers at once and rebases in the background, so the bar says Rebasing… while the request, read
    /// again every `REBASE_EVERY`, says it is; then where merging stands is shown and `Event::Rebased` has the tab read
    /// its commits, or GitLab's reason it failed comes as `Event::Failed`.
    pub fn run_rebase(&mut self, host: Arc<dyn Host>, skip_ci: bool, cx: &mut Context<Self>) {
        use crate::merge::{REBASE_EVERY, REBASE_READS, Rebased, rebased};
        let Some(iid) = self.current else {
            return;
        };
        self.rows.keep_skip_ci(skip_ci);
        self.merge_busy = Some(crate::bar::REBASING);
        self.merge_task = Some(cx.spawn(async move |this, cx| {
            let calling = host.clone();
            let mut result = cx
                .background_executor()
                .spawn(async move { calling.rebase(iid, skip_ci).map(|_| None) })
                .await;
            for _ in 0..REBASE_READS {
                if !matches!(result, Ok(None) | Ok(Some((_, Rebased::Waiting)))) {
                    break;
                }
                cx.background_executor().timer(REBASE_EVERY).await;
                let reading = host.clone();
                result = cx
                    .background_executor()
                    .spawn(async move { reading.merge_state(iid) })
                    .await
                    .map(|state| {
                        let now = rebased(&state);
                        Some((state, now))
                    });
            }
            this.update(cx, |rq, cx| {
                if rq.current != Some(iid) {
                    return;
                }
                rq.merge_busy = None;
                match result {
                    Ok(Some((state, now))) => {
                        rq.merge = Some(state);
                        match now {
                            Rebased::Failed(why) => cx.emit(Event::Failed(why)),
                            Rebased::Done => cx.emit(Event::Rebased(iid)),
                            // Still at it after every read: the page says so (`MergeState::rebasing`).
                            Rebased::Waiting => {}
                        }
                    }
                    Ok(None) => rq.reload_merge(host, cx),
                    Err(e) => {
                        cx.emit(Event::Failed(format!("{e:#}")));
                        rq.reload_merge(host, cx);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    /// A call about merging the open request, `busy` said in Merge's place until it answers; then
    /// where it stands is read again. A refusal comes to the tab as `Event::Failed`.
    pub fn merge_call(
        &mut self,
        busy: &'static str,
        host: Arc<dyn Host>,
        call: impl FnOnce(&dyn Host, u64, &str) -> anyhow::Result<()> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(iid) = self.current else {
            return;
        };
        let node = self
            .merge
            .as_ref()
            .map(|h| h.node.clone())
            .unwrap_or_default();
        self.merge_busy = Some(busy);
        let calling = host.clone();
        self.merge_task = Some(cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { call(&*calling, iid, &node) })
                .await;
            this.update(cx, |rq, cx| {
                if let Err(e) = result {
                    rq.merge_busy = None;
                    cx.emit(Event::Failed(format!("{e:#}")));
                    return cx.notify();
                }
                rq.reload_merge(host, cx);
            })
            .ok();
        }));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artoo_hosts_api::Thread;

    fn thread(id: &str) -> Thread {
        Thread {
            id: id.into(),
            notes: Vec::new(),
            path: None,
            new_line: None,
            old_line: None,
            resolved: false,
            resolvable: false,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        }
    }

    fn base(id: &str) -> Base {
        Base {
            threads: vec![thread(id)],
            ..Default::default()
        }
    }

    #[test]
    fn an_older_read_is_ignored_by_take_base() {
        let mut rq = Requests::default();
        assert!(rq.take_base(3, base("new")));
        assert!(!rq.take_base(2, base("old")));
        assert_eq!(rq.base.threads[0].id, "new");
        assert_eq!(rq.from, 3);
        assert!(rq.take_base(3, base("same")));
    }

    #[test]
    fn a_left_page_is_shown_at_once_on_reopen_and_only_8_are_kept() {
        let mut rq = Requests::default();
        for iid in 1..=10u64 {
            rq.current = Some(iid);
            rq.base = base(&format!("t{iid}"));
            rq.from = iid;
            rq.forget_open();
        }
        assert_eq!(rq.pages.len(), KEPT_PAGES);
        assert!(!rq.restore(1), "the oldest was let go");
        rq.current = Some(10);
        assert!(rq.restore(10));
        assert_eq!(rq.base.threads[0].id, "t10");
        assert_eq!(rq.from, 10);
        assert!(!rq.restore(10), "taken back out");
    }

    use crate::held::tests::mr;
    use artoo_ui_kit::fresh::{Held as Where, Slot};

    #[test]
    fn the_slot_says_what_exists() {
        let mut rq = Requests::default();
        assert_eq!(rq.slot(), Slot::Reading, "nothing yet");
        rq.loading = true;
        assert_eq!(rq.slot(), Slot::Reading, "the first read");
        rq.listed_at = Some(0);
        rq.from_disk = true;
        assert_eq!(rq.slot(), Slot::Refreshing, "a read under a kept copy");
        rq.loading = false;
        assert!(
            matches!(rq.slot(), Slot::Kept(_)),
            "the kept copy, nothing asked"
        );
        rq.from_disk = false;
        assert!(matches!(rq.slot(), Slot::Updated(_)));
        rq.held = Some(crate::held::Held {
            new: vec![3],
            at: Some(Where::NewAbove),
            ..Default::default()
        });
        assert!(matches!(rq.slot(), Slot::Waiting(_, 1, Where::NewAbove)));
        rq.error = Some("Can't reach gitlab.example".into());
        assert!(
            matches!(rq.slot(), Slot::CantReach(_)),
            "rows kept, host away"
        );
        rq.refused = true;
        assert_eq!(rq.slot(), Slot::TokenExpired);
        rq.refused = false;
        rq.listed_at = None;
        assert_eq!(rq.slot(), Slot::CantRead, "nothing kept, read failed");
    }

    #[test]
    fn a_read_holds_what_came_in_and_changes_fields_in_place() {
        let mut rq = Requests {
            list: vec![mr(1, 1, 10), mr(2, 5, 9)],
            ..Default::default()
        };
        rq.take_read(vec![mr(3, 0, 20), mr(1, 2, 12), mr(2, 5, 9)]);
        let ids: Vec<u64> = rq.list.iter().map(|m| m.iid).collect();
        assert_eq!(ids, [1, 2], "the new request waits");
        assert_eq!(rq.list[0].comments, 2, "the field changed in place");
        let held = rq.held.as_ref().expect("held");
        assert_eq!(held.count(), 1);
        // A second read of the same answer holds the same, not twice.
        rq.take_read(vec![mr(3, 0, 20), mr(1, 2, 12), mr(2, 5, 9)]);
        assert_eq!(rq.held.as_ref().map(|h| h.count()), Some(1));
        // What the apply does to the list, apart from reading the facts.
        let held = rq.held.take().expect("held");
        rq.list = held.list;
        assert_eq!(rq.list.len(), 3);
    }

    #[test]
    fn a_request_merged_here_stays_until_applied_and_does_not_count() {
        let mut rq = Requests {
            list: vec![mr(1, 1, 10), mr(2, 5, 9)],
            ..Default::default()
        };
        rq.merged_here.push(2);
        rq.take_read(vec![mr(1, 1, 10)]);
        assert_eq!(rq.list.len(), 2, "the row stays");
        assert!(rq.held.as_ref().is_some_and(|h| h.count() == 0));
        assert!(matches!(rq.slot(), Slot::Updated(_)) || rq.listed_at.is_none());
    }
}
