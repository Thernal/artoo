//! The roots of Branches and Merge requests (#72): the list, the row selected previewed beside it,
//! the list's filter in the page bar (⌘F), and the list's states — loading, empty, no token, an
//! error, offline. Merge requests are grouped by what they need from you, each group a band that
//! folds (#81), and Mine counts the discussions waiting for you and lists them first. Branches list the working tree first, then
//! the local branches with how far they are from the main line and their request; a remote's
//! branches fold under its name.

use super::hover::{CutInTip, Hover};
use super::lists::Field;
use super::nav::BranchRow;
use super::{Workspace, format, icons, island, keys, plural, px, theme, words};
use crate::git::{BranchRef, ChangeKind, RefKind, Repo};
use crate::mr::{Mr, MrState, Thread};
use artoo_branches::roots::{Fact, TreeFile};
pub(super) use artoo_requests::group::Group;
use artoo_requests::list;
use artoo_review_api::Seen;
pub(super) use artoo_ui_kit::controls::skeleton;
use artoo_ui_kit::controls::{empty_state, outline_button, primary_button, progress_badge};
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, FontWeight, Rgba, SharedString, div, prelude::*,
};
use std::collections::HashMap;

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

impl Workspace {
    // ---- reading ------------------------------------------------------------------------

    /// Reads, in the background, when each branch's tip was made and how far it is from the
    /// main line.
    pub(super) fn read_branch_facts(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let branches = self.history.read(cx).branches.clone();
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
    pub(super) fn listed_branches<'a>(
        &self,
        cx: &'a App,
    ) -> impl Iterator<Item = (usize, &'a BranchRef)> {
        self.history
            .read(cx)
            .branches
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind != RefKind::Request)
    }

    /// Branches' rows in the order the list shows them: the working tree, local branches, then
    /// the branches of each open remote; a filter lists the matching branches only.
    pub(super) fn branch_rows(&self, cx: &App) -> Vec<BranchRow> {
        let filter = self.bfilter.to_lowercase();
        if !filter.trim().is_empty() {
            return crate::rank::rank(self.listed_branches(cx), &filter, |(_, b)| &b.name)
                .into_iter()
                .map(|(ix, _)| BranchRow::Branch(ix))
                .collect();
        }
        let mut out = vec![BranchRow::WorkingTree];
        out.extend(
            self.history
                .read(cx)
                .branches
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == RefKind::Local)
                .map(|(ix, _)| BranchRow::Branch(ix)),
        );
        for (remote, ixs) in self.remotes(cx) {
            if self.roots.open_remotes.contains(&remote) {
                out.extend(ixs.into_iter().map(BranchRow::Branch));
            }
        }
        out
    }

    /// The remote branches, by remote.
    fn remotes(&self, cx: &App) -> std::collections::BTreeMap<String, Vec<usize>> {
        let mut out: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
        for (ix, b) in self.history.read(cx).branches.iter().enumerate() {
            if b.kind == RefKind::Remote {
                let remote = b.name.split_once('/').map_or("", |(r, _)| r);
                out.entry(remote.to_owned()).or_default().push(ix);
            }
        }
        out
    }

    /// "4 local · 6 on origin": the count after Branches' name in the bar.
    pub(super) fn branches_count(&self, cx: &App) -> String {
        let local = self
            .history
            .read(cx)
            .branches
            .iter()
            .filter(|b| b.kind == RefKind::Local)
            .count();
        let mut parts = vec![format!("{local} local")];
        parts.extend(
            self.remotes(cx)
                .into_iter()
                .map(|(remote, ixs)| format!("{} on {remote}", ixs.len())),
        );
        parts.join(" · ")
    }

    /// The row Branches selects when nothing is: the working tree with changes, else the branch
    /// checked out.
    pub(super) fn first_branch_row(&self, cx: &App) -> Option<BranchRow> {
        if self.wt.count > 0 {
            return Some(BranchRow::WorkingTree);
        }
        self.history
            .read(cx)
            .branches
            .iter()
            .position(|b| b.is_head)
            .or(self.history.read(cx).branch)
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
        let acts = super::requests::acts_of(cx.entity().downgrade());
        // How far your review of each got, as the review has it.
        let review = self.review.read(cx);
        let heads = shown
            .iter()
            .map(|m| (m.iid, git2::Oid::from_str(&m.head).ok()));
        let seen = heads
            .clone()
            .map(|(iid, head)| (iid, review.request_seen(iid, head)))
            .collect();
        let parts = heads
            .filter_map(|(iid, head)| Some((iid, review.head_part(head?)?)))
            .collect();
        let threads = |iid| self.notices.threads_of(iid);
        let fold = |key, moving: &str, open, row| self.folding(key, moving, open, row);
        let look = list::ListLook {
            shown: &shown,
            picked: self.nav.pick_request,
            filter: &self.bfilter,
            compact: self.compact(),
            rich: self.settings.rich_comments,
            widths: &self.widths,
            seen,
            parts,
            threads: &threads,
            fold: &fold,
        };
        let rq = self.requests.read(cx);
        let banner = list::banner(rq, &acts);
        let body = match list::list(rq, &look, &acts) {
            list::Body::Loading => div()
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
                .into_any_element(),
            list::Body::Empty(empty) => island()
                .flex_1()
                .bg(theme::editor())
                .child(empty)
                .into_any_element(),
            list::Body::List {
                rows,
                preview,
                more,
            } => {
                if let Some(near) = more {
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
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        self.root_list()
                            .border_color(self.zone_border(super::zones::Zone::Branches))
                            .child(rows),
                    )
                    // Its handle widens or narrows the list, kept for every list page (the
                    // developer, 2026-10-23); a double click sets it back.
                    .children((!self.compact()).then(|| self.handle(super::Split::List, cx)))
                    .children((!self.compact()).then(|| {
                        island().flex_1().min_w_0().bg(theme::editor()).child(
                            div()
                                .id("request-preview")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .children(preview),
                        )
                    }))
                    .into_any_element()
            }
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(super::gap()))
            .children(banner)
            .child(body)
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
        let rows = self.branch_rows(cx);
        let filtering = !self.bfilter.trim().is_empty();
        let picked = self
            .nav
            .pick_branch
            .filter(|r| rows.contains(r))
            .or_else(|| self.first_branch_row(cx).filter(|r| rows.contains(r)))
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
                .history
                .read(cx)
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
            for (remote, ixs) in self.remotes(cx) {
                let open = self.roots.open_remotes.contains(&remote);
                let name = remote.clone();
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
            .history
            .read(cx)
            .branches
            .iter()
            .filter(|b| b.kind == RefKind::Local)
            .count();
        let alone = !filtering && local == 1 && self.wt.count == 0;
        let preview = if alone {
            let name = self
                .history
                .read(cx)
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
            .history
            .read(cx)
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
                    let b = &self.history.read(cx).branches[ix];
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
            .history
            .read(cx)
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
        let Some(b) = self.history.read(cx).branches.get(ix) else {
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
