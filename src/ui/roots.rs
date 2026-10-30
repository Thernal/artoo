//! The roots of Branches and Merge requests (#72): the list, the row selected previewed beside it,
//! the list's filter in the page bar (⌘F), and the list's states — loading, empty, no token, an
//! error, offline. Merge requests are grouped by what they need from you, each group a band that
//! folds (#81), and Mine counts the discussions waiting for you and lists them first. Branches list the working tree first, then
//! the local branches with how far they are from the main line and their request; a remote's
//! branches fold under its name.

use super::hover::{CutInTip, Hover};
use super::lists::Field;
use super::nav::BranchRow;
use super::{Workspace, discussion, format, icons, island, keys, plural, px, theme, words};
use crate::git::{BranchRef, ChangeKind, RefKind, Repo};
use crate::mr::{Mr, MrState, Thread};
pub(super) use artoo_requests::group::Group;
use artoo_requests::list;
use artoo_review_api::Seen;
pub(super) use artoo_ui_kit::controls::skeleton;
use artoo_ui_kit::controls::{
    empty_state, outline_button, pill, primary_button, progress_badge, unread_dot,
};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, Rgba, SharedString, Task, div,
    prelude::*,
};
use std::collections::{HashMap, HashSet};

/// The width of a root's list.
impl Workspace {
    /// A root's list: beside its preview at its width, or narrow the whole page (#80).
    pub(super) fn root_list(&self) -> Div {
        if self.compact() {
            island().flex_1().min_w_0()
        } else {
            island().w(px(list_width(self.layout.list))).flex_none()
        }
    }
}

/// A root's list's width: the layout's, or a snapshot's `ARTOO_SNAPSHOT_LIST` to see its rows
/// narrow (#79).
pub(super) fn list_width(width: f32) -> f32 {
    #[cfg(feature = "snapshot")]
    if let Some(w) = std::env::var("ARTOO_SNAPSHOT_LIST")
        .ok()
        .and_then(|w| w.parse().ok())
    {
        return w;
    }
    width
}

/// A branch, as its row and preview say it.
#[derive(Clone)]
pub struct Fact {
    /// When its tip was committed, its subject and who wrote it.
    pub time: i64,
    pub summary: String,
    pub author: String,
    /// What it is counted against (the main line, or its upstream for the main line itself) and
    /// how far apart: (name, ahead, behind).
    pub against: Option<(String, usize, usize)>,
}

/// A file of the working tree, for its row and preview.
#[derive(Clone)]
pub struct TreeFile {
    pub change: ChangeKind,
    pub path: String,
    pub added: usize,
    pub removed: usize,
}

#[derive(Default)]
pub struct Roots {
    /// Per refname.
    pub(super) facts: HashMap<String, Fact>,
    facts_task: Option<Task<()>>,
    /// The remotes whose branches the list shows; the rest are folded to their name.
    open_remotes: HashSet<String>,
    /// The working tree's files as last read, with the fingerprint they were read at.
    tree: Option<(u64, Vec<TreeFile>)>,
    tree_task: Option<Task<()>>,
}

impl Workspace {
    // ---- reading ------------------------------------------------------------------------

    /// Reads, in the background, when each branch's tip was made and how far it is from the
    /// main line.
    pub(super) fn read_branch_facts(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let branches = self.branches.clone();
        self.roots.facts_task = Some(cx.spawn(async move |this, cx| {
            let facts = cx
                .background_executor()
                .spawn(async move {
                    let repo = Repo::open(&root).ok()?;
                    let main = repo.default_branch();
                    let tip = |name: &str| branches.iter().find(|b| b.name == name).map(|b| b.tip);
                    let main_tip = main
                        .as_deref()
                        .and_then(|m| tip(m).or_else(|| tip(&format!("origin/{m}"))));
                    let facts: HashMap<String, Fact> = branches
                        .iter()
                        .filter_map(|b| {
                            let commit = repo.commit(b.tip).ok()?;
                            let against = match (b.kind, &main, main_tip) {
                                (RefKind::Remote, ..) => None,
                                (RefKind::Local, Some(m), _) if *m == b.name => repo.tracking(m),
                                (RefKind::Local, Some(m), Some(t)) => repo
                                    .ahead_behind(b.tip, t)
                                    .map(|(ahead, behind)| (m.clone(), ahead, behind)),
                                _ => None,
                            };
                            let fact = Fact {
                                time: commit.time,
                                summary: commit.summary,
                                author: commit.author,
                                against,
                            };
                            Some((b.refname.clone(), fact))
                        })
                        .collect();
                    Some(facts)
                })
                .await;
            this.update(cx, |this, cx| {
                if let Some(facts) = facts {
                    this.roots.facts = facts;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Reads the working tree's files again when they changed since the last read.
    pub(super) fn read_tree_files(&mut self, cx: &mut Context<Self>) {
        let hash = self.wt.hash;
        if self.wt.count == 0 || self.roots.tree.as_ref().is_some_and(|t| t.0 == hash) {
            return;
        }
        let Some(root) = self.root.clone() else {
            return;
        };
        let settings = self.settings(cx);
        self.roots.tree_task = Some(cx.spawn(async move |this, cx| {
            let files = cx
                .background_executor()
                .spawn(async move {
                    let tree = Repo::open(&root)?.working_tree(settings, false)?;
                    anyhow::Ok(
                        tree.files
                            .iter()
                            .map(|f| TreeFile {
                                change: f.change,
                                path: f.path().to_owned(),
                                added: f.added,
                                removed: f.removed,
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(files) = files {
                    this.roots.tree = Some((hash, files));
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// The request picked in the list: its pipeline and approvals, once per update of it.
    pub(super) fn read_preview(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self
            .nav
            .pick_request
            .and_then(|iid| self.requests.read(cx).find(iid))
            .cloned()
        else {
            return;
        };
        let (Some(remote), Some(root)) = (self.web.clone(), self.root.clone()) else {
            return;
        };
        let host = super::host_of(&remote, cx);
        // Pushed since your review: what changed since the push you viewed.
        let head = git2::Oid::from_str(&m.head).ok();
        let viewed = match self.review.read(cx).request_seen(m.iid, head) {
            Seen::Earlier => head.zip(self.review.read(cx).reviewed_head(m.iid, head)),
            _ => None,
        };
        let settings = self.settings(cx);
        self.requests.update(cx, |rq, cx| {
            rq.read_preview(&m, viewed, host, root, settings, cx)
        });
    }

    // ---- what the lists hold ---------------------------------------------------------------

    /// Request `m`'s group in the list.
    pub(super) fn group_of(&self, m: &Mr, cx: &App) -> Group {
        let head = git2::Oid::from_str(&m.head).ok();
        let seen = self.review.read(cx).request_seen(m.iid, head);
        self.requests.read(cx).group_of(m, seen)
    }

    /// The discussions of request `iid` waiting for its author (`waiting`). Read with the
    /// notifications (`notices`), so only for requests read there.
    fn waiting_threads(&self, iid: u64, cx: &App) -> Vec<&Thread> {
        self.requests
            .read(cx)
            .waiting_for_me(self.notices.threads_of(iid).unwrap_or_default())
    }

    /// The branches a search lists, by index: a request's fetched head is none of them (#114).
    pub(super) fn listed_branches(&self) -> impl Iterator<Item = (usize, &BranchRef)> {
        self.branches
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind != RefKind::Request)
    }

    /// Branches' rows in the order the list shows them: the working tree, local branches, then
    /// the branches of each open remote; a filter lists the matching branches only.
    pub(super) fn branch_rows(&self) -> Vec<BranchRow> {
        let filter = self.bfilter.to_lowercase();
        if !filter.trim().is_empty() {
            return crate::rank::rank(self.listed_branches(), &filter, |(_, b)| &b.name)
                .into_iter()
                .map(|(ix, _)| BranchRow::Branch(ix))
                .collect();
        }
        let mut out = vec![BranchRow::WorkingTree];
        out.extend(
            self.branches
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == RefKind::Local)
                .map(|(ix, _)| BranchRow::Branch(ix)),
        );
        for (remote, ixs) in self.remotes() {
            if self.roots.open_remotes.contains(remote) {
                out.extend(ixs.into_iter().map(BranchRow::Branch));
            }
        }
        out
    }

    /// The remote branches, by remote.
    fn remotes(&self) -> std::collections::BTreeMap<&str, Vec<usize>> {
        let mut out: std::collections::BTreeMap<&str, Vec<usize>> = Default::default();
        for (ix, b) in self.branches.iter().enumerate() {
            if b.kind == RefKind::Remote {
                let remote = b.name.split_once('/').map_or("", |(r, _)| r);
                out.entry(remote).or_default().push(ix);
            }
        }
        out
    }

    /// "4 local · 6 on origin": the count after Branches' name in the bar.
    pub(super) fn branches_count(&self) -> String {
        let local = self
            .branches
            .iter()
            .filter(|b| b.kind == RefKind::Local)
            .count();
        let mut parts = vec![format!("{local} local")];
        parts.extend(
            self.remotes()
                .into_iter()
                .map(|(remote, ixs)| format!("{} on {remote}", ixs.len())),
        );
        parts.join(" · ")
    }

    /// The row Branches selects when nothing is: the working tree with changes, else the branch
    /// checked out.
    pub(super) fn first_branch_row(&self) -> Option<BranchRow> {
        if self.wt.count > 0 {
            return Some(BranchRow::WorkingTree);
        }
        self.branches
            .iter()
            .position(|b| b.is_head)
            .or(self.branch)
            .map(BranchRow::Branch)
    }

    /// A root list's row picked, by a click or ↑ ↓.
    pub(super) fn pick_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        self.nav.pick_request = Some(iid);
        self.requests.read(cx).rows.reveal.set(true);
        self.read_preview(cx);
        cx.notify();
    }

    fn toggle_remote(&mut self, remote: String, cx: &mut Context<Self>) {
        if !self.roots.open_remotes.remove(&remote) {
            self.roots.open_remotes.insert(remote);
        }
        cx.notify();
    }

    fn toggle_group(&mut self, g: Group, cx: &mut Context<Self>) {
        self.mark_motion(&format!("group-{}", g.key()), cx);
        self.requests.update(cx, |rq, _| rq.rows.toggle(g));
        cx.notify();
    }

    /// Whether group `g`'s rows are folded away; a filter typed opens every group.
    fn folds(&self, g: Group, cx: &App) -> bool {
        let filtering = !self.bfilter.trim().is_empty();
        self.requests.read(cx).rows.folds(g, filtering)
    }

    /// The requests the list has a row for, in its order: those of folded groups left out, but
    /// the picked one, which stays under its band.
    pub(super) fn listed_requests(&self, cx: &App) -> Vec<Mr> {
        let past = self.requests.read(cx).view != MrState::Opened;
        self.shown_requests(cx)
            .into_iter()
            .filter(|m| {
                past || self.nav.pick_request == Some(m.iid)
                    || !self.folds(self.group_of(m, cx), cx)
            })
            .collect()
    }

    /// Whether you have been inside request `iid`: opened here, or files of it marked (#81).
    pub(super) fn opened_before(&self, iid: u64, cx: &App) -> bool {
        let head = self
            .requests
            .read(cx)
            .find(iid)
            .and_then(|m| git2::Oid::from_str(&m.head).ok());
        self.requests.read(cx).rows.opened(iid)
            || !matches!(self.review.read(cx).request_seen(iid, head), Seen::Never)
    }

    // ---- the pages ------------------------------------------------------------------------

    /// Merge requests' root: the list grouped, the picked one previewed; or one of its states.
    pub(super) fn render_requests_root(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.requests.read(cx).connectable {
            return island()
                .flex_1()
                .bg(theme::editor())
                .child(self.render_connect(cx))
                .into_any_element();
        }
        let shown = self.shown_requests(cx);
        // The merged or the closed (#114): a flat list, newest first, read as it is scrolled.
        // What shows of them, copied: a row is drawn with the tab's context.
        struct PastShown {
            loading: bool,
            empty: bool,
            more: bool,
            error: Option<String>,
        }
        let past = (self.requests.read(cx).view != MrState::Opened).then(|| {
            let rq = self.requests.read(cx);
            let p = rq.past.of_scope(rq.scope);
            PastShown {
                loading: p.loading,
                empty: p.list.is_empty(),
                more: p.more,
                error: p.error.clone(),
            }
        });
        let loading = match &past {
            Some(p) => p.loading && p.empty,
            None => self.requests.read(cx).loading && self.requests.read(cx).list.is_empty(),
        };
        let body = if loading {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .gap(px(super::gap()))
                .child(
                    island()
                        .w(px(list_width(self.layout.list)))
                        .flex_none()
                        .p(px(16.))
                        .child(skeleton("requests-loading", 7)),
                )
                .child(
                    island()
                        .flex_1()
                        .bg(theme::editor())
                        .px(px(28.))
                        .py(px(26.))
                        .child(skeleton("request-preview-loading", 3)),
                )
                .into_any_element()
        } else if shown.is_empty() && past.as_ref().is_none_or(|p| !p.more) {
            island()
                .flex_1()
                .bg(theme::editor())
                .child(self.requests_empty(cx))
                .into_any_element()
        } else {
            let groups: Vec<Group> = match &past {
                Some(_) => vec![Group::New; shown.len()],
                None => shown.iter().map(|m| self.group_of(m, cx)).collect(),
            };
            let picked = self
                .nav
                .pick_request
                .and_then(|iid| shown.iter().find(|m| m.iid == iid))
                .or_else(|| {
                    shown
                        .iter()
                        .zip(&groups)
                        .find(|(_, g)| past.is_some() || !self.folds(**g, cx))
                        .map(|(m, _)| m)
                })
                .cloned();
            // Each group a band, its rows under it unless it is folded (#81).
            let mut rows: Vec<AnyElement> = Vec::new();
            let mut bands: Vec<(usize, Group, usize)> = Vec::new();
            let mut picked_row = None;
            for (n, (m, &g)) in shown.iter().zip(&groups).enumerate() {
                if past.is_some() {
                    if picked.as_ref().is_some_and(|p| p.iid == m.iid) {
                        picked_row = Some(rows.len());
                    }
                    rows.push(self.request_row(m, g, cx).into_any_element());
                    continue;
                }
                if n == 0 || groups[n - 1] != g {
                    let count = groups.iter().filter(|&&o| o == g).count();
                    bands.push((rows.len(), g, count));
                    let top = rows.is_empty();
                    rows.push(self.group_band(g, count, false, top, cx).into_any_element());
                }
                let is_picked = picked.as_ref().is_some_and(|p| p.iid == m.iid);
                // Each row folds with its group, in size and alpha (#110): the group opens and
                // shuts as one, and every row stays a child of its own for the held band.
                let Some(row) = self.folding(
                    format!("request-row-{}", m.iid),
                    &format!("group-{}", g.key()),
                    !self.folds(g, cx) || is_picked,
                    self.request_row(m, g, cx),
                ) else {
                    continue;
                };
                if is_picked {
                    picked_row = Some(rows.len());
                }
                rows.push(row);
            }
            // More of them while there may be: a shimmer row at the end, read when it comes into
            // sight (#114).
            if let Some(error) = past.as_ref().and_then(|p| p.error.as_ref()) {
                rows.push(
                    div()
                        .px(px(16.))
                        .py(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(super::hover::cut("past-error", error.clone()).flex_1())
                        .child(
                            div()
                                .id("past-retry")
                                .child(outline_button("Retry"))
                                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.retry_past_page(cx)
                                })),
                        )
                        .into_any_element(),
                );
            } else if past.as_ref().is_some_and(|p| p.more || p.loading) {
                rows.push(
                    div()
                        .id("past-more")
                        .px(px(16.))
                        .py(px(12.))
                        .child(super::motion::shimmer("past-more-bars", 2))
                        .into_any_element(),
                );
            }
            // The rows as the last frame laid them out, when they are these rows: where the list's
            // top is, so the band above it is held there.
            let scroll = self.requests.read(cx).rows.scroll.clone();
            let laid = scroll.children_count() == rows.len();
            if past.as_ref().is_some_and(|p| p.more && !p.loading) {
                // Not laid out yet: looked at again once it is. A row's bounds are where it is
                // in the unscrolled list.
                let near = laid
                    && scroll.bounds_for_item(rows.len() - 1).is_some_and(|b| {
                        b.top() + scroll.offset().y < scroll.bounds().bottom() + px(list::BAND * 4.)
                    });
                if near || !laid {
                    let this = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        this.update(cx, |this, cx| {
                            if near {
                                this.load_past_page(this.requests.read(cx).scope, cx);
                            } else {
                                cx.notify();
                            }
                        })
                        .ok();
                    });
                }
            }
            let top = f32::from(scroll.bounds().top() - scroll.offset().y);
            let top_of = |ix: usize| scroll.bounds_for_item(ix).map(|b| f32::from(b.top()));
            if self.requests.read(cx).rows.reveal.take()
                && let Some(ix) = picked_row
            {
                // A row going up out of sight would stop under the held band: the one above it
                // comes into view instead, and with it the row.
                let under_band = laid && ix > 0 && top_of(ix).is_some_and(|t| t < top + list::BAND);
                scroll.scroll_to_item(if under_band { ix - 1 } else { ix });
            }
            let tops: Vec<f32> = bands.iter().filter_map(|&(ix, ..)| top_of(ix)).collect();
            let held = (laid && tops.len() == bands.len())
                .then(|| list::held_band(&tops, top))
                .flatten()
                .map(|(at, push)| {
                    let (_, g, count) = bands[at];
                    div()
                        .absolute()
                        .top(px(push))
                        .left_0()
                        .right_0()
                        .child(self.group_band(g, count, true, true, cx))
                });
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(
                    self.root_list()
                        .border_color(self.zone_border(super::zones::Zone::Branches))
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .overflow_hidden()
                                .child(self.widths.measure("requests"))
                                .child(
                                    div()
                                        .id("requests")
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_y_scroll()
                                        .track_scroll(&scroll)
                                        .pb(px(6.))
                                        .children(rows),
                                )
                                .children(held),
                        ),
                )
                // Its handle widens or narrows the list, kept for every list page (the developer,
                // 2026-10-23); a double click sets it back.
                .children((!self.compact()).then(|| self.handle(super::Split::List, cx)))
                .children((!self.compact()).then(|| {
                    island().flex_1().min_w_0().bg(theme::editor()).child(
                        div()
                            .id("request-preview")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .children(picked.as_ref().map(|m| self.request_preview(m, cx))),
                    )
                }))
                .into_any_element()
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(super::gap()))
            .children(self.list_banner(cx))
            .child(body)
            .into_any_element()
    }

    /// No request to list: why, per filter, and the way to more.
    fn requests_empty(&self, cx: &mut Context<Self>) -> Div {
        use super::requests::Scope;
        let all = self.requests.read(cx).list.len();
        let filter = self.bfilter.trim().to_owned();
        let view = self.requests.read(cx).view;
        let show_all = |label: &'static str| {
            div()
                .id("empty-show-all")
                .child(outline_button(label))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.requests
                        .update(cx, |rq, _| rq.scope = super::requests::Scope::All);
                    this.bfilter.clear();
                    this.field = None;
                    cx.notify();
                }))
                .into_any_element()
        };
        if !filter.is_empty() {
            return empty_state(
                "search",
                format!("Nothing matches “{filter}”"),
                words::say("The filter looks in the number, title, author and branch of each merge request.")
                    .into_owned(),
                Some(show_all("Clear the filter")),
            );
        }
        // The merged or the closed (#114): none read, or none of this filter's.
        if view != MrState::Opened {
            let word = view.word().to_lowercase();
            let past = self
                .requests
                .read(cx)
                .past
                .of_scope(self.requests.read(cx).scope);
            if let Some(error) = &past.error {
                return empty_state(
                    "pull-request",
                    words::say(&format!("The {word} merge requests could not be read"))
                        .into_owned(),
                    error.clone(),
                    Some(
                        div()
                            .id("past-retry")
                            .child(outline_button("Retry"))
                            .on_click(
                                cx.listener(|this, _: &ClickEvent, _, cx| this.retry_past_page(cx)),
                            )
                            .into_any_element(),
                    ),
                );
            }
            let read = !self
                .requests
                .read(cx)
                .past
                .of_scope(Scope::All)
                .list
                .is_empty();
            return match self.requests.read(cx).scope {
                Scope::All => empty_state(
                    "pull-request",
                    words::say(&format!("No {word} merge requests")).into_owned(),
                    words::say(&format!("Merge requests {word} in this project land here."))
                        .into_owned(),
                    None,
                ),
                Scope::ReviewMe => empty_state(
                    "pull-request",
                    format!("None {word} asked for your review"),
                    words::say(&format!(
                        "{} merge requests you were asked to review land here.",
                        view.word()
                    ))
                    .into_owned(),
                    read.then(|| show_all("Show all")),
                ),
                Scope::Mine => empty_state(
                    "pull-request",
                    format!("None of yours is {word}"),
                    words::say(&format!(
                        "The merge requests you opened land here once {word}."
                    ))
                    .into_owned(),
                    read.then(|| show_all("Show all")),
                ),
            };
        }
        if self.requests.read(cx).error.is_some() && all == 0 {
            return empty_state(
                "pull-request",
                words::say("No merge requests yet").into_owned(),
                "They come when the list can be read again.".to_owned(),
                None,
            );
        }
        match self.requests.read(cx).scope {
            Scope::ReviewMe => empty_state(
                "pull-request",
                "Nothing to review".to_owned(),
                format!(
                    "Requests that ask for your review land here. {} open under All.",
                    if all == 1 {
                        "1 is".to_owned()
                    } else {
                        format!("{all} are")
                    }
                ),
                (all > 0).then(|| show_all("Show all")),
            ),
            Scope::Mine => empty_state(
                "pull-request",
                "None of yours is open".to_owned(),
                words::say(
                    "The merge requests you open land here, with what waits for you on each.",
                )
                .into_owned(),
                (all > 0).then(|| show_all("Show all")),
            ),
            Scope::All => empty_state(
                "pull-request",
                words::say("No open merge requests").into_owned(),
                words::say("When someone opens a merge request in this project, it lands here.")
                    .into_owned(),
                None,
            ),
        }
    }

    /// A group's band in the requests list: its icon, name and count, and the chevron that folds
    /// it (a click anywhere on it does); `held` is its copy held at the top of the list (#81).
    fn group_band(
        &self,
        g: Group,
        count: usize,
        held: bool,
        top: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let (icon, tone) = g.icon();
        // At the list's top its fill follows the island's round corners, and the island's own
        // border is its top edge.
        let corner = if super::flat() {
            0.
        } else {
            super::ISLAND_RADIUS - 1.
        };
        let foldable = self.bfilter.trim().is_empty();
        let folded = self.folds(g, cx);
        div()
            .id((if held { "group-held" } else { "group-band" }, g as usize))
            .h(px(list::BAND))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(16.))
            .pr(px(10.))
            .bg(theme::base())
            .when(top, |s| s.rounded_t(px(corner)))
            .when(!top, |s| s.border_t_1())
            .border_b_1()
            .border_color(theme::island_border())
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(if folded {
                theme::muted()
            } else {
                theme::text()
            })
            .child(icons::icon(icon).size(px(14.)).text_color(tone))
            .child(super::hover::cut("heading", g.heading()).flex_1())
            .child(
                div()
                    .flex_none()
                    .px(px(6.))
                    .rounded_full()
                    .bg(theme::hover())
                    .text_size(px(11.))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme::muted())
                    .child(count.to_string()),
            )
            .when(foldable, |s| {
                s.cursor_pointer()
                    .child(
                        icons::icon(if folded { "chevron-right" } else { "chevron" })
                            .size(px(14.))
                            .text_color(theme::faint()),
                    )
                    .on_click(
                        // A double click folds or opens it once, as one click does.
                        cx.listener(move |this, e: &ClickEvent, _, cx| {
                            if e.click_count() < 2 {
                                this.toggle_group(g, cx)
                            }
                        }),
                    )
            })
    }

    /// A request's row: unread dot, number and title, when; who, the pipeline, its discussions.
    fn request_row(&self, m: &Mr, g: Group, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let iid = m.iid;
        let picked = self.nav.pick_request == Some(iid);
        let head = git2::Oid::from_str(&m.head).ok();
        let part = head.and_then(|h| self.review.read(cx).head_part(h));
        let pipeline = self
            .requests
            .read(cx)
            .rows
            .preview(iid)
            .and_then(|p| p.pipeline.clone());
        let waiting = (g == Group::Yours).then(|| self.waiting_threads(iid, cx).len());
        let threads = self.notices.threads_of(iid).map(<[Thread]>::len);
        // A dot: a row of yours a thread waits on, another you have never opened; how far your
        // review is, its group says (#81).
        let unread = m.state == MrState::Opened
            && match waiting {
                Some(n) => n > 0,
                None => !self.opened_before(iid, cx),
            };
        // Its anatomy (#79): the number, the title and the pipeline's verdict always; who (or
        // what waits for you) and when while the list has 260 px; the threads and how far your
        // review is under 360 px leave first. The tooltip holds it all.
        let room = self.widths.room("requests");
        let who = match waiting {
            Some(n) if n > 0 => Some((
                format!(
                    "{} for you",
                    if n == 1 {
                        "1 thread waits".to_owned()
                    } else {
                        format!("{n} threads wait")
                    }
                ),
                theme::warning(),
            )),
            _ => Some((
                if m.draft {
                    format!("draft · {}", m.author)
                } else {
                    m.author.clone()
                },
                theme::muted(),
            )),
        };
        let verdict = pipeline.as_ref().map(artoo_requests::bar::pipeline_word);
        let threads = threads.filter(|&n| n > 0 && waiting.is_none_or(|w| w == 0));
        let mut tip = vec![
            format!("{} {}", words::number(iid), m.title),
            format!("{} · {} → {}", m.author, m.source, m.target),
        ];
        if let Some((glyph, word, _)) = &verdict {
            tip.push(format!("pipeline {glyph} {word}"));
        }
        if let Some(n) = threads {
            tip.push(plural(n, "thread"));
        }
        if let Some((text, _)) = who.as_ref().filter(|_| waiting.is_some_and(|w| w > 0)) {
            tip.push(text.clone());
        }
        tip.push(format!("updated {}", super::hover::full_date(m.updated)));
        let mut meta: Vec<AnyElement> = Vec::new();
        if let Some((text, color)) = who.filter(|_| room.second()) {
            meta.push(div().text_color(color).child(text).into_any_element());
        }
        if let Some((glyph, word, color)) = verdict {
            meta.push(
                div()
                    .text_color(color)
                    .child(format!("{glyph} {word}"))
                    .into_any_element(),
            );
        }
        if let Some(n) = threads.filter(|_| room.third()) {
            meta.push(div().child(plural(n, "thread")).into_any_element());
        }
        let mut line2: Vec<AnyElement> = Vec::new();
        for (n, part) in meta.into_iter().enumerate() {
            if n > 0 {
                line2.push(
                    div()
                        .text_color(theme::faint())
                        .child("·")
                        .into_any_element(),
                );
            }
            line2.push(part);
        }
        div().px(px(6.)).py(px(1.)).child(
            div()
                .id(("mr-row", iid))
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                        this.request_context(iid, e.position, cx)
                    }),
                )
                .px(px(10.))
                .py(px(8.))
                .flex()
                .items_start()
                .gap(px(10.))
                .rounded(px(super::ROW_RADIUS))
                .cursor_pointer()
                .when(picked, |s| s.bg(theme::selected()))
                .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
                .child(unread_dot(unread))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_none()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(words::number(iid)),
                                )
                                .child(div().flex_1().min_w_0().cut_in_tip().child(m.title.clone()))
                                .children(
                                    part.filter(|_| room.third())
                                        .map(|(done, total)| progress_badge(done, total)),
                                )
                                .children(room.second().then(|| {
                                    div()
                                        .flex_none()
                                        .text_size(px(11.))
                                        .text_color(theme::faint())
                                        .child(format::ago(m.updated))
                                })),
                        )
                        .when(!line2.is_empty(), |s| {
                            s.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(px(12.))
                                    .text_color(theme::muted())
                                    .children(line2),
                            )
                        }),
                )
                .tip(tip.join("\n"))
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    // A second click on the picked one opens it, as ↵ does; with no preview
                    // beside the list, the first (#80).
                    let open = this.nav.pick_request == Some(iid) && event.click_count() > 1;
                    if open || this.compact() {
                        this.field = None;
                        return this.select_request(iid, cx);
                    }
                    this.zone = super::zones::Zone::Branches;
                    this.pick_request(iid, cx);
                })),
        )
    }

    /// The request picked: who and where, its title and description, how far your review is,
    /// and Open; for one of yours, the discussions waiting for you first.
    fn request_preview(&self, m: &Mr, cx: &mut Context<Self>) -> AnyElement {
        let iid = m.iid;
        let me = self.requests.read(cx).login.as_deref().unwrap_or_default();
        let mine = !me.is_empty() && m.author_login == me;
        let (pipeline, approvals, since) = self
            .requests
            .read(cx)
            .rows
            .preview(iid)
            .map(|p| (p.pipeline.clone(), p.approvals.clone(), p.since))
            .unwrap_or_default();
        let mut pills: Vec<AnyElement> = Vec::new();
        if let Some(p) = &pipeline {
            let (glyph, word, color) = artoo_requests::bar::pipeline_word(p);
            pills.push(pill(format!("{glyph} {word}"), color).into_any_element());
        }
        if let Some(a) = &approvals {
            let said = if a.required > 0 {
                Some(format!(
                    "{}/{} approved",
                    a.required - a.left.min(a.required),
                    a.required
                ))
            } else {
                (!a.by.is_empty()).then(|| format!("{} approved", a.by.len()))
            };
            pills.extend(said.map(|s| {
                pill(
                    s,
                    if a.left == 0 && !a.by.is_empty() {
                        theme::added()
                    } else {
                        theme::muted()
                    },
                )
                .into_any_element()
            }));
        }
        let props = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(
                div()
                    .font_family(theme::code_font())
                    .text_color(theme::accent())
                    .child(words::number(iid)),
            )
            .child(discussion::avatar(&m.author, mine, 18.))
            .child(
                div()
                    .flex()
                    .gap(px(6.))
                    .child(if mine {
                        "yours".to_owned()
                    } else {
                        m.author.clone()
                    })
                    .child(div().text_color(theme::faint()).child("·"))
                    .child(
                        div()
                            .font_family(theme::code_font())
                            .child(format!("{} → {}", m.source, m.target)),
                    ),
            )
            .child(div().flex_1())
            .children(pills);
        let title = div()
            .text_size(px(19.))
            .line_height(px(26.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(m.title.clone());
        let description = crate::markdown::plain(&m.description);
        let description = description.trim();
        // How far your review is, and what is open.
        let head = git2::Oid::from_str(&m.head).ok();
        let mut facts: Vec<String> = Vec::new();
        match (
            self.review.read(cx).request_seen(iid, head),
            head.and_then(|h| self.review.read(cx).head_part(h)),
        ) {
            (Seen::Done, _) => facts.push("Reviewed at this push".to_owned()),
            (Seen::Earlier, _) => facts.push(match since {
                Some((viewed, 0)) => format!("Pushed since you viewed v{viewed}, no file changed"),
                Some((viewed, n)) => {
                    format!("{} changed since you viewed v{viewed}", plural(n, "file"))
                }
                None => "Pushed since your review".to_owned(),
            }),
            (_, Some((done, total))) => facts.push(format!("{done} of {total} files reviewed")),
            (Seen::Started | Seen::Never, None) => {}
        }
        if let Some(threads) = self.notices.threads_of(iid) {
            let open = threads
                .iter()
                .filter(|t| t.resolvable && !t.resolved)
                .count();
            if open > 0 {
                facts.push(format!("{} open", plural(open, "discussion")));
            }
        }
        if m.draft {
            facts.push("Draft".to_owned());
        }
        // A merged or closed one: when it was (its last update, as the list gives it, #114).
        facts.push(match m.state {
            MrState::Opened => format!("updated {}", format::ago(m.updated)),
            s => format!("{} {}", s.word().to_lowercase(), format::ago(m.updated)),
        });
        let waiting = if mine {
            self.waiting_threads(iid, cx)
        } else {
            Vec::new()
        };
        let first = waiting.first().and_then(|t| t.notes.last().map(|n| n.id));
        let open_label = if first.is_some() {
            "Open the first"
        } else {
            "Open"
        };
        let open_hint = if first.is_some() {
            words::say("opens the merge request at that discussion").into_owned()
        } else {
            "opens on its changes, at the first file not reviewed · ⌥↓ the next not reviewed"
                .to_owned()
        };
        let waits = (!waiting.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .pb(px(6.))
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .child(format!("Waiting for you · {}", waiting.len())),
                )
                .children(waiting.iter().enumerate().map(|(n, t)| {
                    let last = t.notes.last();
                    let who = last.map(|n| n.author.clone()).unwrap_or_default();
                    let gist = last.map(|n| self.one_line(&n.body)).unwrap_or_default();
                    let at = match (&t.path, t.new_line.or(t.old_line)) {
                        (Some(path), Some(line)) => {
                            format!("{}:{line}", path.rsplit('/').next().unwrap_or(path))
                        }
                        (Some(path), None) => path.rsplit('/').next().unwrap_or(path).to_owned(),
                        _ => "General".to_owned(),
                    };
                    let note = last.map(|n| n.id);
                    div()
                        .id(("waiting", n))
                        .flex()
                        .items_start()
                        .gap(px(10.))
                        .py(px(8.))
                        .border_t_1()
                        .border_color(theme::island_border())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .child(discussion::avatar(&who, false, 18.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .text_size(px(12.))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(6.))
                                        .child(div().font_weight(FontWeight::SEMIBOLD).child(who))
                                        .child(
                                            div()
                                                .text_color(theme::faint())
                                                .child(format!("on {at}")),
                                        ),
                                )
                                .child(
                                    super::hover::cut(
                                        SharedString::from(format!("waiting-text-{n}")),
                                        gist,
                                    )
                                    .text_color(theme::muted()),
                                ),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.open_at_note(iid, note, cx)
                        }))
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(28.))
            .py(px(22.))
            .max_w(px(820.))
            .child(props)
            .child(title)
            .children(waits)
            .children((!description.is_empty()).then(|| {
                super::hover::cut_to(
                    SharedString::from(format!("mr-description-{iid}")),
                    description.to_owned(),
                    5,
                )
                .text_size(px(13.))
                .line_height(px(20.))
                .text_color(theme::muted())
            }))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(14.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .children(facts.into_iter().map(|f| div().child(f))),
            )
            .child(
                div().flex().items_center().gap(px(12.)).child(
                    primary_button("request-open", open_label, "↵")
                        .tip(open_hint)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| match first {
                            Some(note) => this.open_at_note(iid, Some(note), cx),
                            None => {
                                this.field = None;
                                this.select_request(iid, cx)
                            }
                        })),
                ),
            )
            .into_any_element()
    }

    /// Opens request `iid` at note `note` (a discussion waiting for you), else as ↵ does.
    pub(super) fn open_at_note(&mut self, iid: u64, note: Option<u64>, cx: &mut Context<Self>) {
        self.field = None;
        match note {
            Some(note) => {
                self.since_on_open = Some((iid, note));
                self.set_zone(super::zones::Zone::Diff, cx);
                self.go_to(
                    crate::links::Target::Request {
                        iid,
                        note: Some(note),
                        line: None,
                    },
                    cx,
                );
            }
            None => self.select_request(iid, cx),
        }
    }

    /// ↵ on Merge requests' root: the picked request opens — for one of yours with discussions
    /// waiting, at the first of them.
    pub(super) fn open_picked_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        let me = self.requests.read(cx).login.clone().unwrap_or_default();
        let mine = self
            .requests
            .read(cx)
            .find(iid)
            .is_some_and(|m| !me.is_empty() && m.author_login == me);
        let note = mine
            .then(|| {
                self.waiting_threads(iid, cx)
                    .first()
                    .and_then(|t| t.notes.last().map(|n| n.id))
            })
            .flatten();
        self.open_at_note(iid, note, cx);
    }

    /// Branches' root: the working tree, the local branches, the remotes folded; the row picked
    /// previewed.
    pub(super) fn render_branches_root(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self.branch_rows();
        let filtering = !self.bfilter.trim().is_empty();
        let picked = self
            .nav
            .pick_branch
            .filter(|r| rows.contains(r))
            .or_else(|| self.first_branch_row().filter(|r| rows.contains(r)))
            .or(rows.first().copied());
        let mut list: Vec<AnyElement> = Vec::new();
        if filtering {
            list.extend(rows.iter().map(|row| {
                self.branch_row(*row, picked == Some(*row), true, cx)
                    .into_any_element()
            }));
            if rows.is_empty() {
                list.push(
                    div()
                        .px(px(16.))
                        .py(px(10.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!("No branch matches “{}”.", self.bfilter.trim()))
                        .into_any_element(),
                );
            }
        } else {
            list.push(heading("Working tree".to_owned()).into_any_element());
            list.push(
                self.branch_row(
                    BranchRow::WorkingTree,
                    picked == Some(BranchRow::WorkingTree),
                    false,
                    cx,
                )
                .into_any_element(),
            );
            let local: Vec<usize> = self
                .branches
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == RefKind::Local)
                .map(|(ix, _)| ix)
                .collect();
            list.push(heading(format!("Local · {}", local.len())).into_any_element());
            list.extend(local.into_iter().map(|ix| {
                let row = BranchRow::Branch(ix);
                self.branch_row(row, picked == Some(row), false, cx)
                    .into_any_element()
            }));
            for (remote, ixs) in self.remotes() {
                let open = self.roots.open_remotes.contains(remote);
                let name = remote.to_owned();
                list.push(
                    div()
                        .id(SharedString::from(format!("remote-{remote}")))
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .px(px(12.))
                        .pt(px(10.))
                        .pb(px(4.))
                        .text_size(px(11.))
                        .text_color(theme::faint())
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::text()))
                        .child(
                            icons::icon(if open { "chevron" } else { "chevron-right" })
                                .size(px(14.))
                                .text_color(theme::faint()),
                        )
                        .child(format!(
                            "{remote} · {}{}",
                            ixs.len(),
                            if open { "" } else { " · folded" }
                        ))
                        .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                            if e.click_count() < 2 {
                                this.toggle_remote(name.clone(), cx)
                            }
                        }))
                        .into_any_element(),
                );
                if open {
                    list.extend(ixs.into_iter().map(|ix| {
                        let row = BranchRow::Branch(ix);
                        self.branch_row(row, picked == Some(row), false, cx)
                            .into_any_element()
                    }));
                }
            }
        }
        let local = self
            .branches
            .iter()
            .filter(|b| b.kind == RefKind::Local)
            .count();
        let alone = !filtering && local == 1 && self.wt.count == 0;
        let preview = if alone {
            let name = self
                .branches
                .iter()
                .find(|b| b.kind == RefKind::Local)
                .map(|b| b.name.clone())
                .unwrap_or_default();
            empty_state(
                "branch",
                format!("Only {name}, nothing ahead"),
                "Branches you make in your terminal or editor show up here, with what they hold and their request.".to_owned(),
                None,
            )
            .into_any_element()
        } else {
            match picked {
                Some(BranchRow::WorkingTree) => self.tree_preview(cx).into_any_element(),
                Some(BranchRow::Branch(ix)) => self.branch_preview(ix, cx),
                None => div().into_any_element(),
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(
                self.root_list()
                    .relative()
                    .border_color(self.zone_border(super::zones::Zone::Branches))
                    .child(self.widths.measure("branches"))
                    .child(
                        div()
                            .id("branches")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .pb(px(6.))
                            .children(list),
                    ),
            )
            // Its handle widens or narrows the list, kept for every list page (the developer,
            // 2026-10-23); a double click sets it back.
            .children((!self.compact()).then(|| self.handle(super::Split::List, cx)))
            .children((!self.compact()).then(|| {
                island().flex_1().min_w_0().bg(theme::editor()).child(
                    div()
                        .id("branch-preview")
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .overflow_y_scroll()
                        .child(preview),
                )
            }))
            .into_any_element()
    }

    /// A row of Branches: the working tree's, or a branch's name, when, how far from the main
    /// line, its request. `flat` (filtered) names a remote branch with its remote. Its anatomy
    /// (#79): the name and ↑ ↓ always; its request and when while the list has 260 px; HEAD and
    /// how far your review is under 360 px leave first; its card holds it all.
    fn branch_row(
        &self,
        row: BranchRow,
        picked: bool,
        flat: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let room = self.widths.room("branches");
        let head_name = self
            .branches
            .iter()
            .find(|b| b.is_head)
            .map(|b| b.name.clone());
        let (id, icon, line1, line2): (SharedString, &'static str, AnyElement, Option<AnyElement>) =
            match row {
                BranchRow::WorkingTree => {
                    let files = self.roots.tree.as_ref().map(|t| &t.1);
                    let line1 = div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(if self.wt.count == 0 {
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Clean")
                                .into_any_element()
                        } else {
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(plural(self.wt.count, "change"))
                                .into_any_element()
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .cut_in_tip()
                                .text_color(theme::muted())
                                .child(if self.wt.count == 0 {
                                    "nothing to commit"
                                } else {
                                    "not committed"
                                }),
                        )
                        .children(head_name.filter(|_| room.second()).map(|n| {
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format!("on {n}"))
                        }))
                        .into_any_element();
                    let line2 = files.filter(|f| !f.is_empty()).map(|f| {
                        let names: Vec<&str> = f.iter().take(2).map(|f| f.path.as_str()).collect();
                        let more = f.len().saturating_sub(2);
                        div()
                            .cut_in_tip()
                            .child(if more > 0 {
                                format!("{} and {more} more", names.join(", "))
                            } else {
                                names.join(", ")
                            })
                            .into_any_element()
                    });
                    ("branch-row-wt".into(), "files", line1, line2)
                }
                BranchRow::Branch(ix) => {
                    let b = &self.branches[ix];
                    let name = match (b.kind, flat) {
                        (RefKind::Remote, false) => {
                            b.name.split_once('/').map_or(b.name.as_str(), |(_, n)| n)
                        }
                        _ => b.name.as_str(),
                    };
                    let fact = self.roots.facts.get(&b.refname);
                    let line1 = div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            super::hover::cut(
                                SharedString::from(format!("branch-name-{ix}")),
                                name.to_owned(),
                            )
                            .flex_1()
                            .font_family(theme::code_font())
                            .when(b.kind == RefKind::Local, |s| {
                                s.font_weight(FontWeight::SEMIBOLD)
                            })
                            .when(b.kind == RefKind::Remote, |s| s.text_color(theme::muted())),
                        )
                        .children(
                            self.review
                                .read(cx)
                                .head_part(b.tip)
                                .filter(|_| room.third())
                                .map(|(done, total)| progress_badge(done, total)),
                        )
                        .children(
                            (room.third() && self.review.read(cx).head_reviewed(b.tip)).then(
                                || {
                                    div()
                                        .text_size(px(11.))
                                        .text_color(theme::added())
                                        .child("✓ reviewed")
                                },
                            ),
                        )
                        .children(fact.filter(|_| room.second()).map(|f| {
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format::ago(f.time))
                        }))
                        .into_any_element();
                    let mut parts: Vec<AnyElement> = Vec::new();
                    if b.is_head && room.third() {
                        parts.push(
                            div()
                                .px(px(6.))
                                .rounded_full()
                                .border_1()
                                .border_color(theme::accent())
                                .text_size(px(10.))
                                .text_color(theme::accent())
                                .child("HEAD")
                                .into_any_element(),
                        );
                    }
                    match fact.and_then(|f| f.against.as_ref()) {
                        Some((up, 0, 0)) => parts.push(
                            div()
                                .child(format!("up to date with {up}"))
                                .into_any_element(),
                        ),
                        Some((_, ahead, behind)) => parts.push(
                            div()
                                .child(format!("↑{ahead} ↓{behind}"))
                                .into_any_element(),
                        ),
                        None => {}
                    }
                    if let Some(m) = self.request_of_branch(ix, cx).filter(|_| room.second()) {
                        let iid = m.iid;
                        parts.push(
                            div()
                                .id(SharedString::from(format!("branch-mr-{ix}")))
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .hover(|s| s.underline())
                                .child(format!("{} →", words::number(iid)))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.select_request(iid, cx)
                                }))
                                .into_any_element(),
                        );
                    }
                    let line2 = (!parts.is_empty()).then(|| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .children(parts)
                            .into_any_element()
                    });
                    (
                        SharedString::from(format!("branch-row-{ix}")),
                        "branch",
                        line1,
                        line2,
                    )
                }
            };
        let card = match row {
            BranchRow::Branch(ix) => self.branch_card(ix, cx),
            BranchRow::WorkingTree => None,
        };
        // The working tree has no card: its tooltip names its files.
        let tree_tip = (row == BranchRow::WorkingTree).then(|| {
            let mut tip = vec![if self.wt.count == 0 {
                "Clean: nothing to commit".to_owned()
            } else {
                format!("{}, not committed", plural(self.wt.count, "change"))
            }];
            if let Some(files) = self.roots.tree.as_ref().map(|t| &t.1) {
                tip.extend(files.iter().take(12).map(|f| f.path.clone()));
                if files.len() > 12 {
                    tip.push(format!("and {} more", files.len() - 12));
                }
            }
            tip.join("\n")
        });
        div().px(px(6.)).py(px(1.)).child(
            div()
                .id(id)
                .px(px(10.))
                .py(px(7.))
                .flex()
                .items_start()
                .gap(px(10.))
                .rounded(px(super::ROW_RADIUS))
                .cursor_pointer()
                .when(picked, |s| s.bg(theme::selected()))
                .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
                .child(
                    icons::icon(icon)
                        .mt(px(2.))
                        .size(px(15.))
                        .text_color(theme::faint()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(line1)
                        .children(
                            line2.map(|l| {
                                div().text_size(px(12.)).text_color(theme::muted()).child(l)
                            }),
                        ),
                )
                .when_some(card, |s, card| s.card(card))
                .when_some(tree_tip, |s, tip| s.tip(tip))
                .when_some(
                    match row {
                        BranchRow::Branch(ix) => Some(ix),
                        BranchRow::WorkingTree => None,
                    },
                    |s, ix| {
                        s.on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, _, cx| {
                                this.branch_context(ix, event.position, cx)
                            }),
                        )
                    },
                )
                .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                    let open = this.nav.pick_branch == Some(row) && event.click_count() > 1;
                    if open || this.compact() {
                        return this.open_branch_row(row, cx);
                    }
                    this.zone = super::zones::Zone::Branches;
                    this.nav.pick_branch = Some(row);
                    if row == BranchRow::WorkingTree {
                        this.read_tree_files(cx);
                    }
                    cx.notify();
                })),
        )
    }

    /// Opens a row of Branches: the working tree, or the branch.
    pub(super) fn open_branch_row(&mut self, row: BranchRow, cx: &mut Context<Self>) {
        self.field = None;
        match row {
            BranchRow::WorkingTree => self.select_working_tree(cx),
            BranchRow::Branch(ix) => self.open_branch(ix, None, cx),
        }
    }

    /// The working tree picked: what is not committed, read only, and the way to its diff.
    fn tree_preview(&self, cx: &mut Context<Self>) -> Div {
        let head = self
            .branches
            .iter()
            .find(|b| b.is_head)
            .map(|b| b.name.clone());
        let files = self
            .roots
            .tree
            .as_ref()
            .map(|t| t.1.clone())
            .unwrap_or_default();
        let props = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(
                icons::icon("files")
                    .size(px(15.))
                    .text_color(theme::muted()),
            )
            .child(match &head {
                Some(n) => format!("Working tree · on {n}"),
                None => "Working tree".to_owned(),
            })
            .child(div().flex_1())
            .child(
                div()
                    .text_color(theme::faint())
                    .child("read only: commit in your editor or terminal"),
            );
        let title = div()
            .text_size(px(19.))
            .line_height(px(26.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(if self.wt.count == 0 {
                "Nothing to commit".to_owned()
            } else {
                format!("{} not committed", plural(self.wt.count, "change"))
            });
        let list = (!files.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .py(px(4.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .children(files.into_iter().take(40).map(|f| {
                    let (letter, color) = change_letter(f.change);
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(12.))
                        .h(px(26.))
                        .text_size(px(13.))
                        .child(
                            div()
                                .w(px(14.))
                                .text_size(px(11.))
                                .font_weight(FontWeight::BOLD)
                                .text_color(color)
                                .child(letter),
                        )
                        .child(
                            super::hover::cut(
                                SharedString::from(format!("wt-file-{}", f.path)),
                                f.path,
                            )
                            .flex_1(),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(6.))
                                .text_size(px(11.))
                                .child(
                                    div()
                                        .text_color(theme::added())
                                        .child(format!("+{}", f.added)),
                                )
                                .child(
                                    div()
                                        .text_color(theme::removed())
                                        .child(format!("−{}", f.removed)),
                                ),
                        )
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(28.))
            .py(px(22.))
            .max_w(px(820.))
            .child(props)
            .child(title)
            .children(list)
            .children((self.wt.count > 0).then(|| {
                div()
                    .flex()
                    .child(
                        primary_button("tree-open", "Open the diff", "↵").on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                this.field = None;
                                this.select_working_tree(cx)
                            },
                        )),
                    )
            }))
    }

    /// A branch picked: its last commit, how far it is from the main line, its request, Open.
    fn branch_preview(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(b) = self.branches.get(ix) else {
            return div().into_any_element();
        };
        let fact = self.roots.facts.get(&b.refname);
        let request = self
            .request_of_branch(ix, cx)
            .map(|m| (m.iid, m.title.clone()));
        let props = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(
                icons::icon("branch")
                    .size(px(15.))
                    .text_color(theme::muted()),
            )
            .child(match b.kind {
                RefKind::Local if b.is_head => "Local branch · checked out".to_owned(),
                RefKind::Local => "Local branch".to_owned(),
                RefKind::Remote => "Remote branch".to_owned(),
                RefKind::Request => "A merge request's head".to_owned(),
            })
            .child(div().flex_1())
            .children(fact.map(|f| {
                div()
                    .text_color(theme::faint())
                    .child(format!("last commit {}", format::ago(f.time)))
            }));
        let title = div()
            .text_size(px(19.))
            .line_height(px(26.))
            .font_weight(FontWeight::SEMIBOLD)
            .font_family(theme::code_font())
            .child(b.name.clone());
        let mut facts: Vec<String> = Vec::new();
        if let Some((against, ahead, behind)) = fact.and_then(|f| f.against.as_ref()) {
            facts.push(if (*ahead, *behind) == (0, 0) {
                format!("up to date with {against}")
            } else {
                format!(
                    "{} ahead of {against}, {} behind",
                    plural(*ahead, "commit"),
                    behind
                )
            });
        }
        let last = fact.map(|f| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .px(px(12.))
                .py(px(8.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .child(div().text_size(px(13.)).child(f.summary.clone()))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!(
                            "{} · {} · {}",
                            format::short(b.tip),
                            f.author,
                            format::ago(f.time)
                        )),
                )
        });
        let mut actions: Vec<AnyElement> = vec![
            primary_button("branch-open", "Open", "↵")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.open_branch_row(BranchRow::Branch(ix), cx)
                }))
                .into_any_element(),
        ];
        if let Some((iid, _)) = &request {
            let iid = *iid;
            actions.push(
                div()
                    .id("branch-request")
                    .child(outline_button(&format!("{} →", words::number(iid))))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.select_request(iid, cx)
                        }),
                    )
                    .into_any_element(),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .px(px(28.))
            .py(px(22.))
            .max_w(px(820.))
            .child(props)
            .child(title)
            .children(request.map(|(iid, title)| {
                div().text_size(px(13.)).text_color(theme::muted()).child(
                    words::say(&format!(
                        "{} is open for this branch: {title}",
                        words::number(iid)
                    ))
                    .into_owned(),
                )
            }))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(14.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .children(facts.into_iter().map(|f| div().child(f))),
            )
            .children(last)
            .child(div().flex().items_center().gap(px(10.)).children(actions))
            .into_any_element()
    }

    /// The list's filter in the page bar: ⌘F gives it the keyboard.
    pub(super) fn bar_filter(&self, placeholder: &'static str, cx: &Context<Self>) -> AnyElement {
        // Narrow, the field is its icon until it is used (#80).
        if self.compact() && self.field != Some(Field::Branches) && self.bfilter.is_empty() {
            return super::bar::bar_icon(
                "list-filter-icon",
                "search",
                format!("{placeholder}  ⌘F"),
            )
            .on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| this.start_field(Field::Branches, cx)),
            )
            .into_any_element();
        }
        div()
            .flex()
            .items_center()
            .w(px(240.))
            .child(
                self.field_box("list-filter", Field::Branches, placeholder, cx)
                    .flex_1()
                    .when(self.field != Some(Field::Branches), |s| {
                        s.child(keys::caps("⌘F", keys::On::Plain))
                    }),
            )
            .into_any_element()
    }

    /// Over a list whose host cannot be read now: offline (the VPN), or the token refused; the
    /// last list stays under it.
    pub(super) fn list_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let error = self.requests.read(cx).error.as_ref()?;
        let offline = error.starts_with("Can't reach");
        let since = self
            .requests
            .read(cx)
            .listed_at
            .map(|t| format!(" The list is from {}", format::clock(t)));
        let text = match (offline, &since) {
            (true, Some(since)) => format!("{error}{since} and refreshes when it is back."),
            (true, None) => format!("{error} The list comes when it is back."),
            (false, Some(since)) => format!("{error}.{since}."),
            (false, None) => format!("{error}."),
        };
        let token = error.contains("token");
        let color = if offline {
            theme::accent()
        } else {
            theme::removed()
        };
        Some(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(10.))
                .px(px(14.))
                .py(px(8.))
                .rounded(px(if super::flat() {
                    0.
                } else {
                    super::ISLAND_RADIUS
                }))
                .border_1()
                .border_color(color)
                .bg(theme::panel())
                .text_size(px(13.))
                .child(
                    icons::icon(if offline { "info" } else { "alert" })
                        .size(px(16.))
                        .text_color(color),
                )
                .child(div().flex_1().min_w_0().child(text))
                .child(
                    div()
                        .id("banner-action")
                        .child(outline_button(if token {
                            "Renew token…"
                        } else {
                            "Retry"
                        }))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            if token {
                                this.settings_section = super::settings::Section::Requests;
                                this.open_settings(cx);
                            } else {
                                this.load_requests(cx);
                                cx.notify();
                            }
                        })),
                )
                .into_any_element(),
        )
    }
}

/// A group's heading in a list: "Needs your review · 3".
fn heading(text: String) -> Div {
    div()
        .px(px(16.))
        .pt(px(10.))
        .pb(px(4.))
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(text)
}

fn change_letter(change: ChangeKind) -> (&'static str, Rgba) {
    match change {
        ChangeKind::Added => ("A", theme::added()),
        ChangeKind::Deleted => ("D", theme::removed()),
        ChangeKind::Modified => ("M", theme::accent()),
        ChangeKind::Renamed => ("R", theme::renamed()),
        ChangeKind::Copied => ("C", theme::renamed()),
        ChangeKind::TypeChanged => ("T", theme::warning()),
    }
}
