//! What the requests read from their host and keep (#165 R5c): the merged and closed pages, the open request's
//! discussions again after a change, its versions, the reactions on its notes, where merging it stands. Each read
//! is the entity's own task; what the tab does after one comes to it as an `Event`.

use crate::requests::{Event, HostSides, Requests, Scope};
use crate::waiting::Base;
use artoo_core::git::Repo;
use artoo_hosts_api::{self as mr, Host};
use gpui::Context;
use std::path::PathBuf;
use std::sync::Arc;

impl Requests {
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

    /// Reads the discussions, pending comments and approvals of the open request again (after a
    /// change to it) and swaps them in, nothing cleared first; a read that fails leaves what
    /// shows (#139). `Event::Read` tells the tab.
    pub fn reload_threads(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let Some(iid) = self.current else {
            return;
        };
        let from = self.waiting.tick();
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
                if let Ok((threads, drafts, approvals)) = read {
                    let base = Base {
                        threads,
                        drafts: drafts.unwrap_or_else(|| rq.base.drafts.clone()),
                        approvals: approvals.or_else(|| rq.base.approvals.clone()),
                    };
                    rq.take_base(from, base);
                    cx.emit(Event::Read(iid));
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// The host's answer for the open request, from a read begun at tick `from`.
    pub fn take_base(&mut self, from: u64, base: Base) {
        self.base = base;
        self.from = from;
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
