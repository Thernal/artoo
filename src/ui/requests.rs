//! Merge requests in the branches island: their titles and authors, a click that opens the source
//! branch, and the discussions other people left on one, in the review panel. Read-only; GitLab
//! only (see `crate::mr`).

use super::nav::Page;
use super::tools::ToolView;
use super::waiting::Op;
use super::{Header, Selection, Workspace};
use crate::git::Repo;
use crate::mr::{self, Mr};
use artoo_branches::opening::RequestOf;
use artoo_requests::discussions::Listing;
use artoo_requests::requests::{Act, Acts};
pub(super) use artoo_requests::requests::{Past, Requests, Scope, Sides, sides};
pub(super) use artoo_ui_kit::controls::group_heading;
use gpui::{App, ClickEvent, Context, WeakEntity, div, prelude::*};
use std::rc::Rc;
use std::sync::Arc;

impl Workspace {
    /// The list shows the merged or the closed requests, or the open ones again (#114).
    pub(super) fn set_request_view(&mut self, state: mr::MrState, cx: &mut Context<Self>) {
        self.requests.update(cx, |rq, _| rq.view = state);
        self.requests.update(cx, |rq, _| rq.rows.picked = None);
        if state != mr::MrState::Opened && self.requests.read(cx).past.of != state {
            // Every scope's first page at once: the chips count them all.
            let mut past = Past {
                of: state,
                ..Past::default()
            };
            for pages in &mut past.scopes {
                pages.more = true;
            }
            self.requests.update(cx, |rq, _| rq.past = past);
            for scope in [Scope::All, Scope::ReviewMe, Scope::Mine] {
                self.load_past_page(scope, cx);
            }
        }
        cx.notify();
    }

    /// The next page of `scope`'s merged or closed requests (`Requests::load_past_page`).
    pub(super) fn load_past_page(&mut self, scope: Scope, cx: &mut Context<Self>) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.load_past_page(scope, host, cx));
    }

    /// The page of the scope shown that could not be read, again.
    pub(super) fn retry_past_page(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.retry_past_page(host, cx));
    }

    /// What became of requests `iids`, gone from the open list or a link's (`Requests::read_left`).
    pub(super) fn read_left(&mut self, iids: Vec<u64>, cx: &mut Context<Self>) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.read_left(iids, host, cx));
    }

    /// Reads the open merge requests of the repository's project, when a token is configured.
    pub(super) fn load_requests(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = self.web.clone() else {
            self.requests.update(cx, |rq, _| *rq = Requests::default());
            return;
        };
        let host = super::host_of(&remote, cx);
        let available = host.available();
        self.requests.update(cx, |rq, _| rq.available = available);
        // The way picked borrows a token not read yet this run (the CLI's, git's): read first.
        if !available && crate::connect::to_borrow(&remote) {
            self.requests.update(cx, |rq, _| rq.connectable = false);
            self.requests.update(cx, |rq, _| rq.loading = true);
            return self.borrow_token(cx);
        }
        self.requests.update(cx, |rq, _| rq.loading = false);
        self.requests
            .update(cx, |rq, _| rq.connectable = !available);
        if !available {
            self.requests.update(cx, |rq, _| rq.list.clear());
            self.requests.update(cx, |rq, _| rq.error = None);
            // The ways in this computer offers (#142, #156).
            return self.look_for_token(cx);
        }
        self.renew_ssh_token(cx);
        self.requests.update(cx, |rq, cx| rq.read_list(host, cx));
    }

    /// The request whose source branch is branch `ix` (`feat/x` and `origin/feat/x` alike), or
    /// whose head it is.
    pub(super) fn request_of_branch<'a>(&self, ix: usize, cx: &'a App) -> Option<&'a Mr> {
        let b = self.history.read(cx).branches.get(ix)?;
        self.requests.read(cx).of_branch(b)
    }

    /// Reads the current request's discussions, pending comments and approvals again
    /// (`Requests::reload_threads`).
    pub(super) fn reload_threads(&mut self, cx: &mut Context<Self>) {
        let Some(host) = self.host(cx) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.reload_threads(host, cx));
    }

    /// How the tab answers what the requests' views ask (`requests::Act`).
    pub(super) fn request_acts(&self, cx: &Context<Self>) -> Acts {
        acts_of(cx.weak_entity())
    }

    /// The open repository's host, when it has one.
    pub(super) fn host(&self, cx: &App) -> Option<std::sync::Arc<dyn artoo_hosts_api::Host>> {
        self.web.as_ref().map(|remote| super::host_of(remote, cx))
    }

    /// What the requests tell the tab after a read of their own.
    pub(super) fn requests_event(
        &mut self,
        _: gpui::Entity<artoo_requests::requests::Requests>,
        event: &artoo_requests::requests::Event,
        cx: &mut Context<Self>,
    ) {
        use artoo_requests::requests::Event;
        match event {
            Event::Listed => {
                if let Some(ix) = self.history.read(cx).branch {
                    self.sync_request(ix, cx);
                }
                self.requests_listed(cx);
                self.follow_pending_link(cx);
                self.load_notices(cx);
            }
            // A link waiting for one goes on.
            Event::Left => self.follow_pending_link(cx),
            Event::Opened { iid, read } => {
                if *read {
                    self.base_taken(*iid, cx);
                    // A link to a comment waits for the discussions; the diff is in by now or
                    // comes later and tries again.
                    let diff_in = self.showing_request(cx);
                    self.resolve_spot(diff_in, cx);
                }
                self.refresh_rows(cx);
            }
            Event::Read(iid) => self.base_taken(*iid, cx),
            Event::Versions(iid) => {
                self.follow_host_sides(*iid, cx);
                self.apply_mr_versions(cx);
                // Two versions of it waited for them (#71).
                self.finish_pending(cx);
                self.compute_histories(cx);
            }
            Event::Histories => {
                self.refresh_rows(cx);
                self.open_since_on_arrival(cx);
            }
            Event::Awards => self.refresh_rows(cx),
            Event::Merged { iid, merge } => self.merged(*iid, merge, cx),
            Event::Created { iid, source } => self.created(*iid, source, cx),
            Event::Failed(e) => self.error = Some(e.clone().into()),
        }
        cx.notify();
    }

    /// Follows the branch selection: a request opened as one (or being opened) whose source
    /// branch is branch `ix` becomes the current one and its discussions are read. A branch opened
    /// in Branches is a branch: no request rides on it (#71).
    pub(super) fn sync_request(&mut self, ix: usize, cx: &mut Context<Self>) {
        let history = self.history.read(cx);
        let iid = (history.pending_request.as_ref())
            .or(history.open_request.as_ref())
            .map(|r| r.2)
            .filter(|iid| {
                // Its source branch, or a branch at its head (a merged one's, #114).
                self.request_of_branch(ix, cx)
                    .is_some_and(|m| m.iid == *iid)
                    || self
                        .requests
                        .read(cx)
                        .find(*iid)
                        .zip(self.history.read(cx).branches.get(ix))
                        .is_some_and(|(m, b)| {
                            m.state != mr::MrState::Opened && b.tip.to_string() == m.head
                        })
            });
        if iid == self.requests.read(cx).current && iid.is_none() {
            return;
        }
        if iid != self.requests.read(cx).current {
            self.requests.update(cx, |rq, _| rq.forget_open());
            self.draft_at = None;
        }
        self.requests.update(cx, |rq, _| rq.current = iid);
        if let Some(iid) = iid {
            if self
                .requests
                .read(cx)
                .walk
                .is_none_or(|(walked, _)| walked != iid)
                && let Some(m) = self.requests.read(cx).find(iid)
            {
                let task = Some((iid, self.group_of(m, cx)));
                self.requests.update(cx, |rq, _| rq.walk = task);
            }
            self.requests.update(cx, |rq, _| rq.mark_opened(iid));
        }
        self.requests.update(cx, |rq, _| rq.reading = false);
        let (Some(iid), Some(host)) = (iid, self.host(cx)) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.read_open(iid, host, cx));
    }

    /// Opens merge request `iid`: its branch in the lists and, as the diff, what the branch adds to
    /// its target — the diff the discussions and new comments belong to.
    pub(super) fn select_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        let Some(m) = self.requests.read(cx).find(iid).cloned() else {
            return;
        };
        let (source, target) = (m.source.clone(), m.target.clone());
        self.load_request_versions(iid, cx);
        if let Some(m) = self.requests.read(cx).find(iid) {
            let (label, detail) = (m.title.clone(), super::words::number(iid));
            self.remember_place(super::recents::Key::Request(iid), label, detail);
        }
        self.history.update(cx, |h, _| h.pending_request = None);
        // A GitHub pull request from a fork has no branch here; `pr/<n>` is where the hint below
        // fetches its head.
        let github = self.web.as_ref().is_some_and(|w| w.github);
        let of = RequestOf {
            number: iid,
            source: &source,
            target: &target,
            head: &m.head,
            open: m.state == mr::MrState::Opened,
            merged_with: m.merged_with.as_deref(),
            github,
        };
        let found = self.history.read(cx).request_branch(&of);
        let Some((src, sides)) = found else {
            // Not here: its head is fetched from the host, once, and then it opens (#114).
            if self
                .requests
                .update(cx, |rq, _| rq.heads_fetched.insert(iid))
            {
                return self.fetch_request_head(iid, github, cx);
            }
            self.error = Some(
                if github {
                    format!(
                        "#{iid}: its branch {source} is not here (a fork's, or not fetched) — fetch it: git fetch origin pull/{iid}/head:refs/remotes/origin/pr/{iid}"
                    )
                } else {
                    format!(
                        "!{iid}: the branch {source} is not here yet — fetch (Settings → Fetch in the background, or git fetch)."
                    )
                }
                .into(),
            );
            cx.notify();
            return;
        };
        self.error = None;
        // Its page now, before the branch it is read from replaces what is loaded (#71).
        self.arrive(super::nav::Page::Request(iid), cx);
        if let Some(sides) = sides {
            self.history
                .update(cx, |h, _| h.pending_request = Some(sides));
        }
        self.select_branch(src, None, cx);
    }

    /// Fetches request `iid`'s head (its branch is gone, or a fork's), reads the branches again
    /// and opens it; says why when the fetch fails.
    fn fetch_request_head(&mut self, iid: u64, github: bool, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let head = self
            .requests
            .read(cx)
            .find(iid)
            .map(|m| m.head.clone())
            .filter(|h| !h.is_empty());
        let news = format!("Fetching {}…", super::words::number(iid));
        self.toast_info(news.clone().into(), cx);
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    crate::git::fetch::fetch_request(&root, iid, github, head.as_deref())?;
                    Repo::open(&root)
                        .and_then(|r| r.branches())
                        .map_err(|e| format!("{e:#}"))
                })
                .await;
            this.update(cx, |this, cx| {
                this.toast_over(&news, cx);
                match read {
                    Ok(branches) => {
                        this.history
                            .update(cx, |h, _| h.branches = Arc::new(branches));
                        this.select_request(iid, cx);
                    }
                    Err(e) => {
                        this.error = Some(
                            format!(
                                "{}: its head could not be fetched — {e}",
                                super::words::number(iid)
                            )
                            .into(),
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// ⌥⌘L: closes the request on screen, back to the list (as ⌘W on it does).
    pub(super) fn leave_request(&mut self, cx: &mut Context<Self>) {
        if let Some(iid) = self.requests.read(cx).current {
            self.close_open(&super::nav::Item::Request(iid), cx);
        }
    }

    /// The request's state goes: what is loaded is its branch, as a branch.
    pub(super) fn drop_request(&mut self, cx: &mut Context<Self>) {
        self.history.update(cx, |h, _| {
            h.open_request = None;
            h.pending_request = None;
        });
        if let Some(ix) = self.history.read(cx).branch {
            self.sync_request(ix, cx);
        }
    }

    /// ⌥↓: the next merge request to review (`next_request`); ⌥↑: the previous one in the list,
    /// reviewed or not (to look again at what was just done).
    pub(super) fn step_request(&mut self, forward: bool, cx: &mut Context<Self>) {
        let shown = self.shown_requests(cx);
        let n = shown.len();
        if n == 0 {
            return;
        }
        let next = if forward {
            self.next_request(cx)
        } else {
            let at = shown
                .iter()
                .position(|m| Some(m.iid) == self.requests.read(cx).current);
            Some(shown[at.map_or(n - 1, |a| (a + n - 1) % n)].clone())
        };
        let Some(next) = next.map(|m| m.iid) else {
            self.error =
                Some("Every other merge request in the list is reviewed, or yours.".into());
            return cx.notify();
        };
        self.review.update(cx, |rv, _| rv.done_card = false);
        self.select_request(next, cx);
    }

    /// The next request to review below the open one (`Requests::next_request`).
    fn next_request(&self, cx: &App) -> Option<Mr> {
        let review = self.review.read(cx);
        let seen = |m: &Mr| review.request_seen(m.iid, git2::Oid::from_str(&m.head).ok());
        self.requests.read(cx).next_request(&self.bfilter, &seen)
    }

    /// Reads the pushes GitLab kept for request `iid` (`Requests::load_versions`); they become the
    /// versions timeline once the request's diff is open.
    fn load_request_versions(&mut self, iid: u64, cx: &mut Context<Self>) {
        self.requests.update(cx, |rq, _| {
            rq.versions = None;
            rq.host = None;
        });
        let (Some(host), Some(root)) = (self.host(cx), self.root.clone()) else {
            return;
        };
        self.requests
            .update(cx, |rq, cx| rq.load_versions(iid, host, root, cx));
    }

    /// Request `iid` is compared from the base and head its host gives, once they are known
    /// (#145): the sides waiting for its branch to load take them, or the open request is compared
    /// again when its diff would differ. A side that is not here is fetched first, once.
    fn follow_host_sides(&mut self, iid: u64, cx: &mut Context<Self>) {
        let Some(host) = self
            .requests
            .read(cx)
            .host
            .filter(|(i, _)| *i == iid)
            .map(|(_, h)| h)
        else {
            return;
        };
        let history = self.history.read(cx);
        let waiting = history.pending_request.as_ref().is_some_and(|p| p.2 == iid);
        let open = if waiting {
            history.pending_request.clone()
        } else {
            history.open_request.clone().filter(|o| o.2 == iid)
        };
        let (Some((base, head, _)), Some(root)) = (open, self.root.clone()) else {
            return;
        };
        let Ok(repo) = Repo::open(&root) else {
            return;
        };
        match sides(&repo, host, (base.1, head.1)) {
            Sides::Same => {}
            Sides::Missing { base, head } => {
                if self
                    .requests
                    .update(cx, |rq, _| rq.sides_fetched.insert(iid))
                {
                    self.fetch_request_sides(iid, base, head, cx);
                }
            }
            Sides::Host(b, h) => {
                let (base, head) = ((base.0, b), (head.0, h));
                if waiting {
                    self.history
                        .update(cx, |h, _| h.pending_request = Some((base, head, iid)));
                    return;
                }
                let open = Some((base.clone(), head.clone(), iid));
                self.history.update(cx, |h, _| h.open_request = open);
                // The Commits tab is read again; a commit or a run picked in it stays picked.
                let id = |ix: usize| self.history.read(cx).commits.get(ix).map(|c| c.id);
                let picked = match self.history.read(cx).selection {
                    Selection::Commit(ix) => id(ix).map(|a| (a, a)),
                    Selection::Commits { newest, oldest } => id(newest).zip(id(oldest)),
                    _ => None,
                };
                self.history
                    .update(cx, |h, _| h.show_request_commits(&base, &head));
                let at = |id: git2::Oid| {
                    self.history
                        .read(cx)
                        .commits
                        .iter()
                        .position(|c| c.id == id)
                };
                if let Some((a, b)) = picked
                    && let (Some(a), Some(b)) = (at(a), at(b))
                {
                    self.history.update(cx, |h, _| {
                        h.selection = match h.selection {
                            Selection::Commit(_) => Selection::Commit(a),
                            _ => Selection::Commits {
                                newest: a,
                                oldest: b,
                            },
                        }
                    });
                }
                let whole = self.request_loaded(iid, cx)
                    || (self.review.read(cx).diff.is_none()
                        && self.nav_page() == Some(&Page::Request(iid)));
                if self.history.read(cx).selection == Selection::None && whole {
                    self.run_compare(base, head, true, Some(iid), cx);
                }
                cx.notify();
            }
        }
    }

    /// Fetches what request `iid`'s host compares it from and this clone lacks — its target
    /// branch for the base, its head for the head —, then compares it from them; says why when
    /// the fetch fails.
    fn fetch_request_sides(&mut self, iid: u64, base: bool, head: bool, cx: &mut Context<Self>) {
        let (Some(root), Some(m)) = (self.root.clone(), self.requests.read(cx).find(iid)) else {
            return;
        };
        let target = m.target.clone();
        let github = self.web.as_ref().is_some_and(|w| w.github);
        let sha = self
            .requests
            .read(cx)
            .host
            .and_then(|(_, h)| h.1)
            .map(|h| h.to_string());
        let number = super::words::number(iid);
        let news = if base {
            format!("Fetching {target} for {number}…")
        } else {
            format!("Fetching {number}…")
        };
        self.toast_info(news.clone().into(), cx);
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    if base {
                        crate::git::fetch::fetch_branch(&root, &target)?;
                    }
                    if head {
                        crate::git::fetch::fetch_request(&root, iid, github, sha.as_deref())?;
                    }
                    Repo::open(&root)
                        .and_then(|r| r.branches())
                        .map_err(|e| format!("{e:#}"))
                })
                .await;
            this.update(cx, |this, cx| {
                this.toast_over(&news, cx);
                match read {
                    Ok(branches) => {
                        let open = this
                            .history.read(cx).branch
                            .and_then(|ix| this.history.read(cx).branches.get(ix))
                            .map(|b| b.refname.clone());
                        this.history.update(cx, |h, _| h.branches = Arc::new(branches));
                        this.history.update(cx, |h, _| h.branch =
                            open.and_then(|r| h.branches.iter().position(|b| b.refname == r)));
                        this.follow_host_sides(iid, cx);
                    }
                    Err(e) => {
                        this.error = Some(
                            format!("{number}: what the host compares it from could not be fetched — {e}")
                                .into(),
                        );
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Shows GitLab's versions of the open request in the timeline, when there is more than one
    /// (`History::show_request_versions`).
    pub(super) fn apply_mr_versions(&mut self, cx: &mut App) {
        let requests = &self.requests;
        self.history.update(cx, |h, cx| {
            h.show_request_versions(requests.read(cx).versions.as_ref())
        });
    }

    /// Is the whole request whose discussions are loaded on screen, as its page (#71)? Its keys
    /// (`a`, ⇧N, Submit review) act only then, never on one loaded under another page.
    pub(super) fn showing_request(&self, cx: &App) -> bool {
        self.requests.read(cx).current.is_some_and(|iid| {
            self.request_loaded(iid, cx)
                && self.nav.space == super::spaces::Space::Requests
                && self.nav_page() == Some(&super::nav::Page::Request(iid))
        })
    }

    /// The diff loaded is the whole of request `iid`, whatever page is on screen.
    pub(super) fn request_loaded(&self, iid: u64, cx: &App) -> bool {
        self.requests.read(cx).current == Some(iid)
            && matches!(
                self.review.read(cx).diff.as_ref().map(|d| &d.header),
                Some(Header::Compare { request: Some(i), .. }) if *i == iid
            )
    }

    /// The discussions on lines of the open file: (index, old side, line).
    pub(super) fn request_threads_here(&self, cx: &App) -> Vec<(usize, bool, u32)> {
        self.review
            .read(cx)
            .current_path()
            .map(|path| self.requests.read(cx).request_threads_in(&path))
            .unwrap_or_default()
    }

    /// A discussion of the request, drawn under its line (`artoo_requests::thread`). A click
    /// anywhere gives it the keyboard, so `r` `x` `o` act on it.
    pub(super) fn render_request_thread(
        &self,
        ix: usize,
        indent: f32,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> gpui::AnyElement {
        use artoo_requests::thread::{self, ThreadLook};
        let replying = self.compose.as_ref().is_some_and(|c| {
            matches!(
                c.target,
                super::composer::Target::Reply(i) | super::composer::Target::ThreadNote(i)
                    if i == ix
            )
        });
        let editing = match self.compose.as_ref().map(|c| c.target) {
            Some(super::composer::Target::Edit(
                change @ super::composer::Change::Note { thread, note },
            )) if thread == ix => self
                .edit_box(change, this.clone(), cx)
                .map(|edit_box| (note, edit_box.into_any_element())),
            _ => None,
        };
        let tab = this.clone();
        let card = |name: &str, login: &str| -> super::discussion::Card {
            let card = Rc::new(self.person_card(name, login, tab.clone(), cx));
            Rc::new(move |_, cx| {
                let c = card.clone();
                cx.new(|_| super::hover::PersonCard::clone_of(&c)).into()
            })
        };
        let fold =
            |key: String, moving: &str, open, content| self.folding(key, moving, open, content);
        let look = ThreadLook {
            indent,
            open: self.mr_open(ix, cx),
            focused: self.thread_at == Some(ix),
            target: self.request_target(cx).is_some(),
            latest: self.latest_version(cx),
            selecting: self.selecting(this.clone()),
            rich: self.settings.rich_comments,
            link_base: self.link_base(),
            tight: self.review.read(cx).tight,
            editing,
            replying: replying.then(|| {
                self.reply_editor(this.clone(), cx)
                    .map(IntoElement::into_any_element)
            }),
            composing: self.compose.is_some(),
            mine: self.my_note_in(ix, cx),
            hovered: self.hover_note,
            confirm: self.confirm_note,
            github: self.on_github(),
            marks: &self.motion,
            me: &self.me,
            card: &card,
            fold: &fold,
            moving: self.motion.recent(&format!("thread-{ix}")),
        };
        let acts = acts_of(this.clone());
        let on: thread::OnThread = Rc::new(move |event, cx| {
            this.update(cx, |this, cx| this.thread_event(ix, event, cx))
                .ok();
        });
        thread::render_thread(&self.requests, ix, look, acts, on, cx)
    }

    /// A click on discussion `ix` (`artoo_requests::thread::ThreadEvent`).
    fn thread_event(
        &mut self,
        ix: usize,
        event: artoo_requests::thread::ThreadEvent,
        cx: &mut Context<Self>,
    ) {
        use artoo_requests::thread::ThreadEvent;
        match event {
            ThreadEvent::Fold(open) => {
                self.thread_at = Some(ix);
                self.set_thread_open(ix, open, cx)
            }
            ThreadEvent::Pick => {
                if self.thread_at != Some(ix) {
                    self.thread_at = Some(ix);
                    cx.notify();
                }
            }
            ThreadEvent::ToggleRich => self.toggle_rich_comments(cx),
            ThreadEvent::Hover { note, on } => {
                if on && self.hover_note != Some(note) {
                    self.hover_note = Some(note);
                    cx.notify();
                } else if !on && self.hover_note == Some(note) {
                    self.hover_note = None;
                    cx.notify();
                }
            }
            ThreadEvent::Reply => self.start_reply(ix, cx),
            ThreadEvent::Resolve => self.toggle_resolved(ix, cx),
            ThreadEvent::Edit(note) => {
                self.start_edit(super::composer::Change::Note { thread: ix, note }, cx)
            }
            ThreadEvent::AskDelete(note) => {
                self.confirm_note = Some(note);
                cx.notify()
            }
            ThreadEvent::Delete(note) => self.delete_own_note(ix, note, cx),
            ThreadEvent::KeepNote => {
                self.confirm_note = None;
                cx.notify()
            }
        }
    }

    /// The discussions on lines of the files of this diff, in reading order.
    fn thread_order(&self, cx: &App) -> Vec<usize> {
        let Some(diff) = &self.review.read(cx).diff else {
            return Vec::new();
        };
        let mut order: Vec<(usize, u32, usize)> = self
            .requests
            .read(cx)
            .threads
            .iter()
            .enumerate()
            .filter_map(|(ix, t)| {
                let file = diff
                    .files
                    .iter()
                    .position(|f| Some(f.path()) == t.path.as_deref())?;
                Some((file, t.new_line.or(t.old_line)?, ix))
            })
            .collect();
        order.sort_unstable();
        order.into_iter().map(|(_, _, ix)| ix).collect()
    }

    /// ⇧n / ⇧p: the next or previous discussion of the request, shown in the diff.
    pub(super) fn step_thread(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !self.showing_request(cx) {
            return;
        }
        let order = self.thread_order(cx);
        if order.is_empty() {
            return;
        }
        let at = self
            .thread_at
            .and_then(|t| order.iter().position(|&ix| ix == t));
        let next = match (at, forward) {
            (None, true) => 0,
            (None, false) => order.len() - 1,
            (Some(i), true) => (i + 1) % order.len(),
            (Some(i), false) => (i + order.len() - 1) % order.len(),
        };
        let ix = order[next];
        self.thread_at = Some(ix);
        if !self.mr_open(ix, cx) {
            self.set_thread_open(ix, true, cx);
        }
        let t = &self.requests.read(cx).threads[ix];
        if let Some(path) = t.path.clone() {
            let (line, old) = (t.new_line.or(t.old_line).unwrap_or(1), t.new_line.is_none());
            self.jump_to_line(&path, line, old, cx);
        }
        cx.notify();
    }

    /// The discussion `r` `x` `o` act on: the one with the keyboard (a click or ⇧N gave it), else
    /// the first open one from the top of the view.
    pub(super) fn acting_thread(&self, cx: &App) -> Option<usize> {
        self.thread_at.or_else(|| {
            let top = self.review.read(cx).local_top();
            self.review
                .read(cx)
                .rows
                .iter()
                .skip(top)
                .find_map(|row| match row {
                    super::rows::Row::Request(ix) if self.mr_open(*ix, cx) => Some(*ix),
                    _ => None,
                })
        })
    }

    pub(super) fn reply_focused(&mut self, cx: &mut Context<Self>) {
        // In the Inbox, r answers the selected notice.
        if self.inbox && self.zone != super::zones::Zone::Tool {
            return self.reply_in_inbox(cx);
        }
        if let Some(ix) = self
            .acting_thread(cx)
            .filter(|_| self.request_target(cx).is_some())
        {
            self.thread_at = Some(ix);
            self.start_reply(ix, cx);
        }
    }

    pub(super) fn resolve_focused(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = self
            .acting_thread(cx)
            .filter(|_| self.request_target(cx).is_some())
        {
            self.thread_at = Some(ix);
            self.toggle_resolved(ix, cx);
        }
    }

    /// Is discussion `ix` shown open? A resolved one starts folded, as in GitLab.
    pub(super) fn mr_open(&self, ix: usize, cx: &App) -> bool {
        let default = self
            .requests
            .read(cx)
            .threads
            .get(ix)
            .is_some_and(|t| !t.resolved);
        default != self.toggled_mr.contains(&ix)
    }

    /// Opens or folds discussion `ix`.
    pub(super) fn set_thread_open(&mut self, ix: usize, open: bool, cx: &mut Context<Self>) {
        // Its notes fade in as it opens (#56, 10).
        self.mark_motion(&format!("thread-{ix}"), cx);
        if self.mr_open(ix, cx) != open && !self.toggled_mr.remove(&ix) {
            self.toggled_mr.insert(ix);
        }
        self.refresh_rows(cx);
        cx.notify();
    }

    /// `o`: folds or opens the discussion the keyboard is on.
    pub(super) fn toggle_focused(&mut self, cx: &mut Context<Self>) {
        if let Some(ix) = self.acting_thread(cx) {
            self.thread_at = Some(ix);
            let open = self.mr_open(ix, cx);
            self.set_thread_open(ix, !open, cx);
        }
    }

    /// A pending comment: a dashed frame, only its author sees it until the review is submitted
    /// (`artoo_requests::thread::render_draft`).
    pub(super) fn render_draft(
        &self,
        ix: usize,
        indent: f32,
        this: WeakEntity<Self>,
        cx: &App,
    ) -> gpui::AnyElement {
        use artoo_requests::thread::{self, DraftEvent, DraftLook};
        let Some(id) = self.requests.read(cx).drafts.get(ix).map(|d| d.id) else {
            return div().into_any_element();
        };
        let look = DraftLook {
            indent,
            focused: self.draft_at == Some(ix),
            selecting: self.selecting(this.clone()),
            rich: self.settings.rich_comments,
            link_base: self.link_base(),
            tight: self.review.read(cx).tight,
            one: self.sends_one_draft(),
            agent: self.draft_agent(id, cx),
            me: &self.me,
        };
        let on: thread::OnDraft = Rc::new(move |event, cx| {
            this.update(cx, |this, cx| match event {
                DraftEvent::Send => this.send_draft_now(id, cx),
                DraftEvent::ToggleRich => this.toggle_rich_comments(cx),
                DraftEvent::Delete => this.discard_draft(id, cx),
            })
            .ok();
        });
        thread::render_draft(self.requests.read(cx), ix, look, on, cx)
    }

    /// A change to the open request, `ops` sent with `work`; a refusal says why in the error bar
    /// (#139).
    fn request_action(
        &mut self,
        ops: Vec<Op>,
        work: impl FnOnce(std::sync::Arc<dyn artoo_hosts_api::Host>) -> anyhow::Result<()>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        if let Some(iid) = self.requests.read(cx).current {
            self.send_change(iid, ops, work, Self::say_refused, cx);
        }
    }

    /// Resolves discussion `ix`, or opens it again: it folds, or opens, at once (#139).
    pub(super) fn toggle_resolved(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(t) = self.requests.read(cx).threads.get(ix) else {
            return;
        };
        if super::waiting::is_temp_thread(&t.id) {
            return;
        }
        let (id, resolved) = (t.id.clone(), !t.resolved);
        let Some(iid) = self.requests.read(cx).current else {
            return;
        };
        // Shown as its new state shows it.
        self.mark_motion(&format!("thread-{ix}"), cx);
        self.toggled_mr.remove(&ix);
        let op = Op::Resolve {
            thread: id.clone(),
            resolved,
        };
        self.request_action(vec![op], move |host| host.resolve(iid, &id, resolved), cx);
    }

    /// `a`: approves the open request, or takes one's approval back.
    pub(super) fn toggle_approval(&mut self, cx: &mut Context<Self>) {
        let (Some(iid), Some(a)) = (
            self.requests.read(cx).current,
            self.requests.read(cx).approvals.clone(),
        ) else {
            return;
        };
        if !self.showing_request(cx) {
            return;
        }
        let op = vec![Op::Approve { mine: !a.mine }];
        if a.mine {
            self.request_action(op, move |host| host.unapprove(iid), cx);
        } else if a.can {
            let head = self
                .requests
                .read(cx)
                .find(iid)
                .map(|m| m.head.clone())
                .unwrap_or_default();
            self.request_action(op, move |host| host.approve(iid, &head), cx);
        } else {
            self.error = Some("GitLab does not let you approve this merge request.".into());
            cx.notify();
        }
    }

    /// ⌥N / ⌥P: the next or previous of one's pending comments, across the files of the request.
    pub(super) fn step_draft(&mut self, forward: bool, cx: &mut Context<Self>) {
        if !self.showing_request(cx) {
            return;
        }
        let order = self.draft_order(cx);
        if order.is_empty() {
            return;
        }
        let at = self
            .draft_at
            .and_then(|d| order.iter().position(|&(_, _, ix)| ix == d));
        let n = order.len();
        let next = match (at, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        let ix = order[next].2;
        self.draft_at = Some(ix);
        let d = &self.requests.read(cx).drafts[ix];
        if let Some(path) = d.path.clone() {
            let (line, old) = (d.new_line.or(d.old_line).unwrap_or(1), d.new_line.is_none());
            self.jump_to_line(&path, line, old, cx);
        }
        cx.notify();
    }

    /// The agent that wrote each pending comment an agent wrote, by its id (#150).
    pub(super) fn draft_agents(&self, cx: &App) -> std::collections::HashMap<u64, String> {
        self.requests
            .read(cx)
            .drafts
            .iter()
            .filter_map(|d| Some((d.id, self.draft_agent(d.id, cx)?.to_owned())))
            .collect()
    }

    /// The pending comments on lines of the diff loaded, in its order: (file, line, index).
    pub(super) fn draft_order(&self, cx: &App) -> Vec<(usize, u32, usize)> {
        let Some(diff) = self.review.read(cx).diff.as_ref() else {
            return Vec::new();
        };
        let mut order: Vec<(usize, u32, usize)> = self
            .requests
            .read(cx)
            .drafts
            .iter()
            .enumerate()
            .filter_map(|(ix, d)| {
                let (path, new, old) = self.requests.read(cx).draft_place(d)?;
                let file = diff.files.iter().position(|f| f.path() == path)?;
                Some((file, new.or(old)?, ix))
            })
            .collect();
        order.sort_unstable();
        order
    }

    pub(super) fn discard_draft(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(iid) = self
            .requests
            .read(cx)
            .current
            .filter(|_| !super::waiting::is_temp(id))
        else {
            return;
        };
        self.request_action(
            vec![Op::DropDraft(id)],
            move |host| host.delete_draft(iid, id),
            cx,
        );
    }

    /// Send this now: pending comment `id` is published alone and becomes a discussion on its
    /// line; the others stay pending. A failure leaves it pending and says why (#138).
    pub(super) fn send_draft_now(&mut self, id: u64, cx: &mut Context<Self>) {
        let Some(iid) = self
            .requests
            .read(cx)
            .current
            .filter(|_| !super::waiting::is_temp(id))
        else {
            return;
        };
        let op = self.requests.update(cx, |rq, _| {
            let w = &mut rq.waiting;
            Op::Publish(vec![(id, w.temp_thread(), w.temp())])
        });
        self.request_action(vec![op], move |host| host.publish_draft(iid, id), cx);
    }

    /// Whether a pending comment can be sent alone: GitLab publishes one draft, GitHub only the
    /// whole review (#138).
    pub(super) fn sends_one_draft(&self) -> bool {
        self.web.as_ref().is_some_and(|w| !w.github)
    }

    /// Deletes one's own note `note` of discussion number `thread` on GitLab or GitHub.
    fn delete_own_note(&mut self, thread: usize, note: u64, cx: &mut Context<Self>) {
        self.confirm_note = None;
        let (Some(iid), Some(discussion)) = (
            self.requests.read(cx).current,
            self.requests
                .read(cx)
                .threads
                .get(thread)
                .map(|t| t.id.clone()),
        ) else {
            return;
        };
        self.request_action(
            vec![Op::Delete { note }],
            move |host| host.delete_note(iid, &discussion, note),
            cx,
        );
    }

    /// The Discussions tool window (`Requests::discussions_window`).
    pub(super) fn discussions_tool(&self, cx: &mut Context<Self>) -> ToolView {
        let rq = self.requests.read(cx);
        let look = Listing {
            showing: self.showing_request(cx),
            rich: self.settings.rich_comments,
            github: self.on_github(),
            latest: self.latest_version(cx),
            agents: self.draft_agents(cx),
        };
        let window = rq.discussions_window(look, self.request_acts(cx));
        ToolView {
            title: "Discussions".into(),
            count: window.count,
            actions: Vec::new(),
            body: window.body,
            foot: window.foot,
        }
    }

    /// Every comment of the open diff: the request's discussions and pending ones, and the agent's.
    /// The Discussions window's count: the request's own discussions and pending notes (#76).
    pub(super) fn comments_total(&self, cx: &App) -> usize {
        if self.showing_request(cx) {
            self.requests.read(cx).threads.len() + self.requests.read(cx).drafts.len()
        } else {
            0
        }
    }

    /// The requests the scope and filter let through, in the order the list shows them
    /// (`Requests::shown`).
    pub(super) fn shown_requests(&self, cx: &App) -> Vec<Mr> {
        let review = self.review.read(cx);
        let seen = |m: &Mr| review.request_seen(m.iid, git2::Oid::from_str(&m.head).ok());
        self.requests.read(cx).shown(&self.bfilter, &seen)
    }

    /// All · To review · Mine with their counts, in Merge requests' bar, once the token's user is
    /// known (#70, #71).
    pub(super) fn request_filters(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let loading = self.requests.read(cx).loading && self.requests.read(cx).list.is_empty();
        if self.requests.read(cx).login.is_none() || loading {
            return None;
        }
        // The merged or the closed: as many as the host says, or counted as far as read (#114,
        // #169).
        let past = self.requests.read(cx).view != mr::MrState::Opened;
        let count = |scope: Scope| {
            if !past {
                let list = &self.requests.read(cx).list;
                return list
                    .iter()
                    .filter(|m| self.requests.read(cx).in_scope(m, scope))
                    .count()
                    .to_string();
            }
            let pages = self.requests.read(cx).past.of_scope(scope);
            match pages.total {
                Some(total) => total.to_string(),
                None => format!("{}{}", pages.list.len(), if pages.more { "+" } else { "" }),
            }
        };
        let filter = |scope: Scope, id: &'static str, name: &'static str| {
            artoo_ui_kit::controls::chip(
                id,
                format!("{name} {}", count(scope)),
                self.requests.read(cx).scope == scope,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.requests.update(cx, |rq, _| rq.scope = scope);
                cx.notify();
            }))
        };
        let view = self.requests.read(cx).view;
        let state =
            artoo_ui_kit::controls::chip("request-state", format!("{} ▾", view.word()), past)
                .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                    use super::menu::{Act, Entry};
                    let entries = [
                        mr::MrState::Opened,
                        mr::MrState::Merged,
                        mr::MrState::Closed,
                    ]
                    .into_iter()
                    .map(|s| Entry::new(s.word(), Act::RequestView(s)).checked(s == view))
                    .collect();
                    this.open_menu(e.position(), vec![entries], cx);
                }));
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    artoo_ui_kit::controls::group()
                        .child(filter(Scope::All, "scope-all", "All"))
                        .child(filter(Scope::ReviewMe, "scope-review", "To review"))
                        .child(filter(Scope::Mine, "scope-mine", "Mine")),
                )
                .child(state),
        )
    }

    /// Keeps the typed token, asks GitLab who it is, and starts listing on success.
    pub(super) fn submit_token(&mut self, cx: &mut Context<Self>) {
        let Some(remote) = self.web.clone() else {
            return;
        };
        let host = super::host_of(&remote, cx);
        let token = self.token_input.trim().to_owned();
        if token.len() < 8 || token.contains(char::is_whitespace) {
            self.gitlab_check = Some(Err("That does not look like a token.".to_owned()));
            return cx.notify();
        }
        if let Err(e) = host.save_token(&token) {
            self.gitlab_check = Some(Err(format!("{e:#}")));
            return cx.notify();
        }
        self.gitlab_check = None;
        self.testing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let check = host.clone();
            let result = cx
                .background_executor()
                .spawn(async move { check.me() })
                .await;
            this.update(cx, |this, cx| {
                this.testing = false;
                match result {
                    Ok(who) => {
                        let host = host.api_host();
                        crate::connect::set_note(&host, |n| {
                            n.way = Some("pasted".to_owned());
                            n.ide = None;
                        });
                        this.gitlab_check = Some(Ok(who));
                        this.token_input.clear();
                        this.field = None;
                        this.load_requests(cx);
                    }
                    Err(e) => {
                        host.forget_token();
                        this.gitlab_check = Some(Err(format!("{e:#}")));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Merge requests' first run, a project without a token (#72): the ways in, centred in the
    /// content (#156, `connecting.rs`).
    pub(super) fn render_connect(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        self.render_ways(cx)
    }

    /// Shows `line` of `path`: that file first when it is another one.
    pub(super) fn jump_to_line(
        &mut self,
        path: &str,
        line: u32,
        old: bool,
        cx: &mut Context<Self>,
    ) {
        self.review
            .update(cx, |rv, _| rv.jump_line = Some((line, old)));
        if self.review.read(cx).current_path().as_deref() == Some(path) {
            self.apply_jump(cx);
            return;
        }
        let ix = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .and_then(|d| d.files.iter().position(|f| f.path() == path));
        match ix {
            Some(ix) => self.select_file(ix, cx),
            None => self.review.update(cx, |rv, _| rv.jump_line = None),
        }
    }
}

/// How the tab `this` answers what the requests' views ask (`requests::Act`).
pub(super) fn acts_of(this: WeakEntity<Workspace>) -> Acts {
    Rc::new(move |act, cx| {
        this.update(cx, |this, cx| match act {
            Act::ToggleApproval => this.toggle_approval(cx),
            Act::MergeReasons(at) => this.open_merge_reasons(at, cx),
            Act::OpenMerge => this.open_merge(cx),
            Act::Rebase => this.rebase_request(cx),
            Act::CancelAutoMerge => this.cancel_auto_merge(cx),
            Act::React { note, name } => this.toggle_reaction(note, &name, cx),
            Act::OpenGrid(note) => this.open_reaction_grid(note, cx),
            Act::CloseGrid(note) => {
                if this.requests.read(cx).grid_note() == Some(note) {
                    this.close_reaction_grid(cx)
                }
            }
            Act::ThreadNote(thread) => this.start_thread_note(thread, cx),
            Act::ReplyMentioning { thread, login } => this.reply_mentioning(thread, &login, cx),
            Act::CompareSince(ix) => this.compare_since(ix, cx),
            Act::JumpToLine { path, line, old } => this.jump_to_line(&path, line, old, cx),
            Act::OpenSubmit => this.open_submit(cx),
            Act::RequestContext { iid, at } => this.request_context(iid, at, cx),
            Act::RunSubmit => this.run_submit(cx),
            Act::CreateGo => this.submit_create(cx),
            Act::UseTemplate(ix) => this.use_template(ix, cx),
            Act::PickSource(name) => this.pick_branch(name, true, cx),
            Act::PickTarget(name) => this.pick_branch(name, false, cx),
            Act::SendNow(id) => this.send_draft_now(id, cx),
            Act::RunMerge => this.run_merge(cx),
            Act::ShowNewVersion => this.show_new_version(cx),
            Act::PickRequest(iid) => {
                this.zone = super::zones::Zone::Branches;
                this.pick_request(iid, cx);
            }
            Act::OpenRequest { iid, note } => this.open_at_note(iid, note, cx),
            Act::LeaveRequest => this.leave_request(cx),
            Act::ToggleGroup(g) => {
                this.mark_motion(&format!("group-{}", g.key()), cx);
                this.requests.update(cx, |rq, _| rq.rows.toggle(g));
                cx.notify();
            }
            Act::RetryPast => this.retry_past_page(cx),
            Act::ShowAll => {
                this.requests.update(cx, |rq, _| rq.scope = Scope::All);
                this.bfilter.clear();
                this.field = None;
                cx.notify();
            }
            Act::Relist => {
                this.load_requests(cx);
                cx.notify();
            }
            Act::RenewToken => {
                this.settings_section = super::settings::Section::Requests;
                this.open_settings(cx);
            }
        })
        .ok();
    })
}

#[cfg(test)]
mod tests {
    /// `n` commits in a line from `parent`, each a file of its own; `tag` keeps two lines from
    /// the same parent apart.
    fn line(
        repo: &git2::Repository,
        parent: Option<git2::Oid>,
        n: usize,
        tag: &str,
    ) -> Vec<git2::Oid> {
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let mut out = Vec::new();
        let mut parent = parent;
        for i in 0..n {
            let mut tree = repo.treebuilder(None).unwrap();
            let blob = repo.blob(format!("{tag} {i}").as_bytes()).unwrap();
            tree.insert(format!("f{i}"), blob, 0o100644).unwrap();
            let tree = repo.find_tree(tree.write().unwrap()).unwrap();
            let parents: Vec<_> = parent
                .iter()
                .map(|p| repo.find_commit(*p).unwrap())
                .collect();
            let parents: Vec<_> = parents.iter().collect();
            let id = repo.commit(None, &sig, &sig, "c", &tree, &parents).unwrap();
            out.push(id);
            parent = Some(id);
        }
        out
    }

    #[test]
    fn a_request_is_compared_from_the_hosts_sides_when_this_clone_would_differ() {
        use super::{Sides, sides};
        let dir = tempfile::TempDir::new().unwrap();
        let git = git2::Repository::init(dir.path()).unwrap();
        // The target here is at `a`; on the host it went on to `c`, the request was rebased on
        // it (`d`), and later the target went on again (`e`).
        let main = line(&git, None, 3, "main");
        let (a, c) = (main[0], main[2]);
        let d = line(&git, Some(c), 1, "request")[0];
        let e = line(&git, Some(c), 1, "target")[0];
        let repo = crate::git::Repo::open(dir.path()).unwrap();

        // A target not fetched since: every target commit after `a` would be the request's.
        assert_eq!(sides(&repo, (Some(c), Some(d)), (a, d)), Sides::Host(c, d));
        // Up to date, or ahead: the same merge base, nothing to load again.
        assert_eq!(sides(&repo, (Some(c), Some(d)), (c, d)), Sides::Same);
        assert_eq!(sides(&repo, (Some(c), Some(d)), (e, d)), Sides::Same);
        // Merged by a fast-forward: the head is in the target, which shows nothing.
        let merged = line(&git, Some(d), 1, "merged")[0];
        assert_eq!(
            sides(&repo, (Some(c), Some(d)), (merged, d)),
            Sides::Host(c, d)
        );
        // The source branch here is behind the host's head.
        assert_eq!(sides(&repo, (Some(c), Some(e)), (c, d)), Sides::Host(c, e));
        // What the host names is not here: fetched first.
        let gone = git2::Oid::from_str("1234567890123456789012345678901234567890").unwrap();
        assert_eq!(
            sides(&repo, (Some(gone), Some(d)), (a, d)),
            Sides::Missing {
                base: true,
                head: false
            }
        );
        assert_eq!(
            sides(&repo, (Some(c), Some(gone)), (a, d)),
            Sides::Missing {
                base: false,
                head: true
            }
        );
        // The host gave no base: the target here, with the host's head.
        assert_eq!(sides(&repo, (None, Some(d)), (c, d)), Sides::Same);
    }
}
