//! Merge requests in the branches island: their titles and authors, a click that opens the source
//! branch, and the discussions other people left on one, in the review panel. Read-only; GitLab
//! only (see `crate::mr`).

use super::nav::Page;
use super::{Header, Selection, Workspace};
use crate::git::Repo;
use crate::mr::{self, Mr};
use artoo_branches_api::RequestOf;
use artoo_core::mr::{Sides, sides};
use artoo_hosts_api::connect::{self, Connect, Here};
use artoo_requests_api::Change;
pub(super) use artoo_requests_api::Scope;
use artoo_requests_api::{Act, Acts, Listing};
use gpui::{App, ClickEvent, Context, WeakEntity, div, prelude::*};
use std::rc::Rc;

impl Workspace {
    /// The list shows the merged or the closed requests, or the open ones again (#114).
    pub(super) fn set_request_view(&mut self, state: mr::MrState, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.set_view(state, cx);
        }
        cx.notify();
    }

    /// The next page of `scope`'s merged or closed requests (`Requests::load_past_page`).
    pub(super) fn load_past_page(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.load_past_page(scope, cx);
        }
    }

    /// The page of the scope shown that could not be read, again.
    pub(super) fn retry_past_page(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.retry_past_page(cx);
        }
    }

    /// What became of requests `iids`, gone from the open list or a link's (`Requests::read_left`).
    pub(super) fn read_left(&mut self, iids: Vec<u64>, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.read_left(iids, cx);
        }
    }

    /// Reads the open merge requests of the repository's project, when a token is configured.
    pub(super) fn load_requests(&mut self, cx: &mut Context<Self>) {
        let Some(requests) = self.requests_api() else {
            return;
        };
        let Some(remote) = self.web.clone() else {
            return requests.reset(cx);
        };
        let available = super::host_of(&remote, cx).available();
        // The way picked borrows a token not read yet this run (the CLI's, git's): read first.
        if !available && self.connecting().is_some_and(|c| c.to_borrow(&remote)) {
            requests.set_token(available, false, true, cx);
            return self.borrow_token(cx);
        }
        requests.set_token(available, !available, false, cx);
        if !available {
            requests.clear_list(cx);
            // The ways in this computer offers (#142, #156).
            return self.look_for_token(cx);
        }
        self.renew_ssh_token(cx);
        requests.read_list(cx);
    }

    /// The request whose source branch is branch `ix` (`feat/x` and `origin/feat/x` alike), or
    /// whose head it is.
    pub(super) fn request_of_branch(&self, ix: usize, cx: &App) -> Option<Mr> {
        let list = self.branch_list(cx);
        let (iid, _) = self.requests_api()?.of_branch(list.get(ix)?, cx)?;
        self.find_request(iid, cx)
    }

    /// Reads the current request's discussions, pending comments and approvals again
    /// (`Requests::reload_threads`).
    pub(super) fn reload_threads(&mut self, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.reload_threads(cx);
        }
    }

    /// What the requests tell the tab after a read of their own.
    pub(super) fn requests_event(
        &mut self,
        event: &artoo_requests_api::Event,
        cx: &mut Context<Self>,
    ) {
        use artoo_requests_api::Event;
        match event {
            Event::Listed => {
                if let Some(ix) = self.branch_ix(cx) {
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
            Event::Create(asked) => self.create_asked(asked.clone(), cx),
            Event::Failed(e) => self.error = Some(e.clone().into()),
            Event::Noticed(news) => self.notices_read(news, cx),
        }
        cx.notify();
    }

    /// Follows the branch selection: a request opened as one (or being opened) whose source
    /// branch is branch `ix` becomes the current one and its discussions are read. A branch opened
    /// in Branches is a branch: no request rides on it (#71).
    pub(super) fn sync_request(&mut self, ix: usize, cx: &mut Context<Self>) {
        let (pending, open) = self
            .branches()
            .map(|b| b.request_sides(cx))
            .unwrap_or_default();
        let iid = (pending.as_ref())
            .or(open.as_ref())
            .map(|r| r.2)
            .filter(|iid| {
                // Its source branch, or a branch at its head (a merged one's, #114).
                self.request_of_branch(ix, cx)
                    .is_some_and(|m| m.iid == *iid)
                    || self
                        .find_request(*iid, cx)
                        .zip(self.branch_list(cx).get(ix))
                        .is_some_and(|(m, b)| {
                            m.state != mr::MrState::Opened && b.tip.to_string() == m.head
                        })
            });
        if iid == self.current_request(cx) && iid.is_none() {
            return;
        }
        if iid != self.current_request(cx) {
            self.draft_at = None;
        }
        let Some(requests) = self.requests_api() else {
            return;
        };
        requests.follow(iid, cx);
        if let Some(iid) = iid {
            requests.read_open(iid, cx);
        }
    }

    /// Request `iid` opened as its page: kept in the history, and loaded unless it is (`load_page`,
    /// which reads it through `select_request`).
    pub(super) fn open_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        self.go_page(super::nav::Page::Request(iid), cx);
    }

    /// Opens merge request `iid`: its branch in the lists and, as the diff, what the branch adds to
    /// its target — the diff the discussions and new comments belong to.
    pub(super) fn select_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        let Some(m) = self.find_request(iid, cx) else {
            return;
        };
        let (source, target) = (m.source.clone(), m.target.clone());
        self.load_request_versions(iid, cx);
        if let Some(m) = self.find_request(iid, cx) {
            let (label, detail) = (m.title.clone(), super::words::number(iid));
            self.remember_place(super::recents::Key::Request(iid), label, detail);
        }
        if let Some(branches) = self.branches() {
            branches.set_pending_request(None, cx);
        }
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
        let found = self.branches().and_then(|b| b.request_branch(&of, cx));
        let Some((src, sides)) = found else {
            // Not here: its head is fetched from the host, once, and then it opens (#114).
            if self
                .requests_api()
                .is_some_and(|r| r.fetch_head_once(iid, cx))
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
        if let Some(sides) = sides
            && let Some(branches) = self.branches()
        {
            branches.set_pending_request(Some(sides), cx);
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
            .find_request(iid, cx)
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
                    Ok(list) => {
                        if let Some(branches) = this.branches() {
                            branches.set_list(list, cx);
                        }
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
        if let Some(iid) = self.current_request(cx) {
            self.close_open(&super::nav::Item::Request(iid), cx);
        }
    }

    /// The request's state goes: what is loaded is its branch, as a branch.
    pub(super) fn drop_request(&mut self, cx: &mut Context<Self>) {
        if let Some(branches) = self.branches() {
            branches.set_open_request(None, cx);
            branches.set_pending_request(None, cx);
        }
        if let Some(ix) = self.branch_ix(cx) {
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
                .position(|m| Some(m.iid) == self.current_request(cx));
            Some(shown[at.map_or(n - 1, |a| (a + n - 1) % n)].clone())
        };
        let Some(next) = next.map(|m| m.iid) else {
            self.error =
                Some("Every other merge request in the list is reviewed, or yours.".into());
            return cx.notify();
        };
        self.rv().close_done(cx);
        self.open_request(next, cx);
    }

    /// The next request to review below the open one (`Requests::next_request`).
    fn next_request(&self, cx: &App) -> Option<Mr> {
        self.requests_api()?.next_request(&self.bfilter, cx)
    }

    /// Reads the pushes GitLab kept for request `iid` (`Requests::load_versions`); they become the
    /// versions timeline once the request's diff is open.
    fn load_request_versions(&mut self, iid: u64, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.load_versions(iid, cx);
        }
    }

    /// Request `iid` is compared from the base and head its host gives, once they are known
    /// (#145): the sides waiting for its branch to load take them, or the open request is compared
    /// again when its diff would differ. A side that is not here is fetched first, once.
    fn follow_host_sides(&mut self, iid: u64, cx: &mut Context<Self>) {
        let Some(host) = self.requests_api().and_then(|r| r.host_sides(iid, cx)) else {
            return;
        };
        let Some(branches) = self.branches() else {
            return;
        };
        let (pending, open) = branches.request_sides(cx);
        let waiting = pending.as_ref().is_some_and(|p| p.2 == iid);
        let open = if waiting {
            pending
        } else {
            open.filter(|o| o.2 == iid)
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
                    .requests_api()
                    .is_some_and(|r| r.fetch_sides_once(iid, cx))
                {
                    self.fetch_request_sides(iid, base, head, cx);
                }
            }
            Sides::Host(b, h) => {
                let (base, head) = ((base.0, b), (head.0, h));
                if waiting {
                    branches.set_pending_request(Some((base, head, iid)), cx);
                    return;
                }
                branches.set_open_request(Some((base.clone(), head.clone(), iid)), cx);
                // The Commits tab is read again; a commit or a run picked in it stays picked.
                branches.show_request_commits(&base, &head, cx);
                let whole = self.request_loaded(iid, cx)
                    || (self.shown_diff(cx).is_none()
                        && self.nav_page() == Some(&Page::Request(iid)));
                if self.selection(cx) == Selection::None && whole {
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
        let (Some(root), Some(m)) = (self.root.clone(), self.find_request(iid, cx)) else {
            return;
        };
        let target = m.target.clone();
        let github = self.web.as_ref().is_some_and(|w| w.github);
        let sha = (self.requests_api())
            .and_then(|r| r.host_sides(iid, cx))
            .and_then(|h| h.1)
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
                    Ok(list) => {
                        if let Some(branches) = this.branches() {
                            branches.set_list(list, cx);
                        }
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
        let versions = self.requests_api().and_then(|r| r.versions(cx));
        if let Some(branches) = self.branches() {
            branches.show_request_versions(versions.as_ref(), cx);
        }
    }

    /// Is the whole request whose discussions are loaded on screen, as its page (#71)? Its keys
    /// (`a`, ⇧N, Submit review) act only then, never on one loaded under another page.
    pub(super) fn showing_request(&self, cx: &App) -> bool {
        self.current_request(cx).is_some_and(|iid| {
            self.request_loaded(iid, cx)
                && self.nav.space == super::spaces::Space::Requests
                && self.nav_page() == Some(&super::nav::Page::Request(iid))
        })
    }

    /// The diff loaded is the whole of request `iid`, whatever page is on screen.
    pub(super) fn request_loaded(&self, iid: u64, cx: &App) -> bool {
        self.current_request(cx) == Some(iid)
            && matches!(
                self.shown_diff(cx).as_ref().map(|d| &d.header),
                Some(Header::Compare { request: Some(i), .. }) if *i == iid
            )
    }

    /// The discussions on lines of the open file: (index, old side, line).
    pub(super) fn request_threads_here(&self, cx: &App) -> Vec<(usize, bool, u32)> {
        self.current_path(cx)
            .map(|path| {
                self.requests_api()
                    .map(|r| r.threads_in(&path, cx))
                    .unwrap_or_default()
            })
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
        use artoo_requests_api::{OnThread, ThreadLook};
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
            tight: self.tight(cx),
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
        let on: OnThread = Rc::new(move |event, cx| {
            this.update(cx, |this, cx| this.thread_event(ix, event, cx))
                .ok();
        });
        match self.requests_api() {
            Some(requests) => requests.render_thread(ix, look, on, cx),
            None => div().into_any_element(),
        }
    }

    /// A click on discussion `ix` (`artoo_requests::thread::ThreadEvent`).
    fn thread_event(
        &mut self,
        ix: usize,
        event: artoo_requests_api::ThreadEvent,
        cx: &mut Context<Self>,
    ) {
        use artoo_requests_api::ThreadEvent;
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
        let Some(diff) = &self.shown_diff(cx) else {
            return Vec::new();
        };
        let mut order: Vec<(usize, u32, usize)> = self
            .request_threads(cx)
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
        let t = &self.request_threads(cx)[ix];
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
            let top = self.rv().local_top(cx);
            self.rv()
                .rows(cx)
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
        let default = self.request_thread(ix, cx).is_some_and(|t| !t.resolved);
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
        use artoo_requests_api::{DraftEvent, DraftLook, OnDraft};
        let Some(id) = self.request_drafts(cx).get(ix).map(|d| d.id) else {
            return div().into_any_element();
        };
        let look = DraftLook {
            indent,
            focused: self.draft_at == Some(ix),
            selecting: self.selecting(this.clone()),
            rich: self.settings.rich_comments,
            link_base: self.link_base(),
            tight: self.tight(cx),
            one: self.sends_one_draft(),
            agent: self.draft_agent(id, cx),
            me: &self.me,
        };
        let on: OnDraft = Rc::new(move |event, cx| {
            this.update(cx, |this, cx| match event {
                DraftEvent::Send => this.send_draft_now(id, cx),
                DraftEvent::ToggleRich => this.toggle_rich_comments(cx),
                DraftEvent::Delete => this.discard_draft(id, cx),
            })
            .ok();
        });
        match self.requests_api() {
            Some(requests) => requests.render_draft(ix, look, on, cx),
            None => div().into_any_element(),
        }
    }

    /// A change to the open request; a refusal says why in the error bar (#139).
    fn request_action(&mut self, change: Change, cx: &mut Context<Self>) {
        if let Some(iid) = self.current_request(cx) {
            self.send_change(iid, change, Self::say_refused, cx);
        }
    }

    /// Resolves discussion `ix`, or opens it again: it folds, or opens, at once (#139).
    pub(super) fn toggle_resolved(&mut self, ix: usize, cx: &mut Context<Self>) {
        let Some(t) = self.request_thread(ix, cx) else {
            return;
        };
        if self.is_temp_thread(&t.id) {
            return;
        }
        let (thread, resolved) = (t.id.clone(), !t.resolved);
        // Shown as its new state shows it.
        self.mark_motion(&format!("thread-{ix}"), cx);
        self.toggled_mr.remove(&ix);
        self.request_action(Change::Resolve { thread, resolved }, cx);
    }

    /// `a`: approves the open request, or takes one's approval back.
    pub(super) fn toggle_approval(&mut self, cx: &mut Context<Self>) {
        let (Some(iid), Some(a)) = (
            self.current_request(cx),
            self.requests_api().and_then(|r| r.approvals(cx)).clone(),
        ) else {
            return;
        };
        if !self.showing_request(cx) {
            return;
        }
        if a.mine {
            self.request_action(Change::Unapprove, cx);
        } else if a.can {
            let head = self
                .find_request(iid, cx)
                .map(|m| m.head.clone())
                .unwrap_or_default();
            self.request_action(Change::Approve { head }, cx);
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
        let d = &self.request_drafts(cx)[ix];
        if let Some(path) = d.path.clone() {
            let (line, old) = (d.new_line.or(d.old_line).unwrap_or(1), d.new_line.is_none());
            self.jump_to_line(&path, line, old, cx);
        }
        cx.notify();
    }

    /// The agent that wrote each pending comment an agent wrote, by its id (#150).
    pub(super) fn draft_agents(&self, cx: &App) -> std::collections::HashMap<u64, String> {
        self.request_drafts(cx)
            .iter()
            .filter_map(|d| Some((d.id, self.draft_agent(d.id, cx)?.to_owned())))
            .collect()
    }

    /// The pending comments on lines of the diff loaded, in its order: (file, line, index).
    pub(super) fn draft_order(&self, cx: &App) -> Vec<(usize, u32, usize)> {
        let Some(diff) = self.shown_diff(cx) else {
            return Vec::new();
        };
        let mut order: Vec<(usize, u32, usize)> = (0..self.request_drafts(cx).len())
            .filter_map(|ix| {
                let (path, new, old) = self.requests_api()?.draft_place(ix, cx)?;
                let file = diff.files.iter().position(|f| f.path() == path)?;
                Some((file, new.or(old)?, ix))
            })
            .collect();
        order.sort_unstable();
        order
    }

    pub(super) fn discard_draft(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.is_temp(id) {
            self.request_action(Change::DropDraft(id), cx);
        }
    }

    /// Send this now: pending comment `id` is published alone and becomes a discussion on its
    /// line; the others stay pending. A failure leaves it pending and says why (#138).
    pub(super) fn send_draft_now(&mut self, id: u64, cx: &mut Context<Self>) {
        if !self.is_temp(id) {
            self.request_action(Change::PublishDraft(id), cx);
        }
    }

    /// Whether a pending comment can be sent alone: GitLab publishes one draft, GitHub only the
    /// whole review (#138).
    pub(super) fn sends_one_draft(&self) -> bool {
        self.web.as_ref().is_some_and(|w| !w.github)
    }

    /// Deletes one's own note `note` of discussion number `thread` on GitLab or GitHub.
    fn delete_own_note(&mut self, thread: usize, note: u64, cx: &mut Context<Self>) {
        self.confirm_note = None;
        let Some(thread) = self.request_thread(thread, cx).map(|t| t.id.clone()) else {
            return;
        };
        self.request_action(Change::Delete { thread, note }, cx);
    }

    /// The requests the scope and filter let through, in the order the list shows them
    /// (`Requests::shown`).
    pub(super) fn shown_requests(&self, cx: &App) -> Vec<Mr> {
        self.requests_api()
            .map(|r| r.shown(&self.bfilter, cx))
            .unwrap_or_default()
    }

    /// All · To review · Mine with their counts, in Merge requests' bar, once the token's user is
    /// known (#70, #71).
    pub(super) fn request_filters(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let loading = self.requests_listing(cx) && self.request_list(cx).is_empty();
        if self.request_login(cx).is_none() || loading {
            return None;
        }
        let requests = self.requests_api()?;
        let view = requests.view(cx);
        let past = view != mr::MrState::Opened;
        let count = |scope: Scope| requests.scope_count(scope, cx);
        let filter = |scope: Scope, id: &'static str, name: &'static str| {
            artoo_ui_kit::controls::chip(
                id,
                format!("{name} {}", count(scope)),
                requests.scope(cx) == scope,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if let Some(requests) = this.requests_api() {
                    requests.set_scope(scope, cx);
                }
                cx.notify();
            }))
        };
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
                    .map(|s| {
                        Entry::new(
                            s.word(),
                            Act::new(move |this, _, cx| this.set_request_view(s, cx)),
                        )
                        .checked(s == view)
                    })
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

    /// Merge requests' first run, a project without a token (#72): the ways in, centred in the
    /// content (#156), hosts' connect panel.
    pub(super) fn render_connect(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        match self.connecting() {
            Some(connecting) => connecting.panel(self.here(cx), cx),
            None => div().into_any_element(),
        }
    }

    /// The connect panel of the tab's host (`artoo_hosts_api::connect`), hosts'.
    pub(super) fn connecting(&self) -> Option<Rc<dyn Connect>> {
        self.services.get()
    }

    /// Where the tab is, as the connect panel is handed it.
    pub(super) fn here(&self, cx: &App) -> Here {
        Here {
            root: self.root.clone(),
            web: self.web.clone(),
            typing: self.has_keyboard(super::typing::Input::Part(connect::INPUT), cx),
        }
    }

    /// The connect panel asked once the update under way is over, with where the tab is then: a project
    /// opening reads its requests before the tab holds its root.
    fn ask_connecting(
        &self,
        ask: impl FnOnce(&dyn Connect, Here, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(connecting) = self.connecting() else {
            return;
        };
        let tab = cx.weak_entity();
        cx.defer(move |cx| {
            if let Some(here) = tab.upgrade().map(|tab| tab.read(cx).here(cx)) {
                ask(&*connecting, here, cx);
            }
        });
    }

    /// No token for the host, or Settings → Merge requests opened: the ways in are looked for.
    pub(super) fn look_for_token(&mut self, cx: &mut Context<Self>) {
        self.ask_connecting(|c, here, cx| c.look(here, cx), cx);
    }

    /// The way picked for the host borrows a token not read yet this run: it is read, then the
    /// requests are (`connected`).
    fn borrow_token(&mut self, cx: &mut Context<Self>) {
        self.ask_connecting(|c, here, cx| c.borrow(here, cx), cx);
    }

    /// The token Artoo made renewed a few days before its end.
    fn renew_ssh_token(&mut self, cx: &mut Context<Self>) {
        if let Some(connecting) = self.connecting() {
            connecting.renew(self.here(cx), cx);
        }
    }

    /// A way in connected the host: the requests read again; a toast saying with what, its button
    /// opening Settings → Merge requests.
    pub(super) fn connected(&mut self, said: Option<String>, cx: &mut Context<Self>) {
        self.load_requests(cx);
        if let Some(text) = said {
            let toast = artoo_shell_api::Toast::new(text);
            self.show_toast(
                toast.button("Settings", super::settings::RequestsSettings),
                cx,
            );
        }
    }

    /// Shows `line` of `path`: that file first when it is another one.
    pub(super) fn jump_to_line(
        &mut self,
        path: &str,
        line: u32,
        old: bool,
        cx: &mut Context<Self>,
    ) {
        self.rv().set_jump_line(Some((line, old)), cx);
        if self.current_path(cx).as_deref() == Some(path) {
            self.apply_jump(cx);
            return;
        }
        let ix = self
            .shown_diff(cx)
            .as_ref()
            .and_then(|d| d.files.iter().position(|f| f.path() == path));
        match ix {
            Some(ix) => self.select_file(ix, cx),
            None => self.rv().set_jump_line(None, cx),
        }
    }
}

/// How the tab `this` answers what the requests' views ask (`requests::Act`).
/// What the requests' Discussions window asks of the tab (`artoo_requests_api::DiscussionsHost`), and whether the
/// request's diff was on screen as the tab began to draw (`Workspace::discussions_showing`).
pub(super) struct DiscussionsHost(pub WeakEntity<Workspace>, pub Rc<std::cell::Cell<bool>>);

impl artoo_requests_api::DiscussionsHost for DiscussionsHost {
    fn showing(&self, _: &App) -> bool {
        self.1.get()
    }

    fn listing(&self, cx: &App) -> Listing {
        let Some(tab) = self.0.upgrade() else {
            return Listing {
                showing: false,
                rich: false,
                github: false,
                latest: None,
                agents: Default::default(),
            };
        };
        let this = tab.read(cx);
        Listing {
            showing: self.1.get(),
            rich: this.settings.rich_comments,
            github: this.on_github(),
            latest: this.latest_version(cx),
            agents: this.draft_agents(cx),
        }
    }
}

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
                if this.requests_api().and_then(|r| r.grid_note(cx)) == Some(note) {
                    this.close_reaction_grid(cx)
                }
            }
            Act::ThreadNote(thread) => this.start_thread_note(thread, cx),
            Act::ReplyMentioning { thread, login } => this.reply_mentioning(thread, &login, cx),
            Act::CompareSince(ix) => this.compare_since(ix, cx),
            Act::JumpToLine { path, line, old } => this.jump_to_line(&path, line, old, cx),
            Act::StepThread { down } => this.step_thread(down, cx),
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
            Act::SinceStep { a, b } => this.go_since(a, b, cx),
            Act::SinceAll => this.leave_since(cx),
            Act::ToggleGroup(g) => {
                if let Some(requests) = this.requests_api() {
                    let key = requests.toggle_group(g, cx);
                    this.mark_motion(&key, cx);
                }
                cx.notify();
            }
            Act::RetryPast => this.retry_past_page(cx),
            Act::ShowAll => {
                if let Some(requests) = this.requests_api() {
                    requests.set_scope(Scope::All, cx);
                }
                this.bfilter.clear();
                this.field = None;
                cx.notify();
            }
            Act::Relist => {
                this.load_requests(cx);
                cx.notify();
            }
            Act::RenewToken => {
                this.settings_section = super::settings::REQUESTS;
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
