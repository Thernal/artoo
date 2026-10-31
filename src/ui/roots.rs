//! The roots of Branches and Merge requests (#72): the list, the row selected previewed beside it,
//! the list's filter in the page bar (⌘F), and the list's states — loading, empty, no token, an
//! error, offline. Merge requests are grouped by what they need from you, each group a band that
//! folds (#81), and Mine counts the discussions waiting for you and lists them first. Branches list the working tree first, then
//! the local branches with how far they are from the main line and their request; a remote's
//! branches fold under its name. Each list's rows and preview are their feature's
//! (`artoo_branches::list`, `artoo_requests::list`); the tab draws the islands around them.

use super::lists::Field;
use super::nav::BranchRow;
use super::{Workspace, island, keys, px, theme};
use crate::git::BranchRef;
use crate::mr::{Mr, MrState, Thread};
use artoo_branches::list::{self as branches, Beside, Card, ListLook};
pub(super) use artoo_requests::group::Group;
use artoo_requests::list;
use artoo_review_api::Seen;
pub(super) use artoo_ui_kit::controls::skeleton;
use gpui::{AnyElement, App, ClickEvent, Context, Div, div, prelude::*};

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

    /// Reads when each branch's tip was made and how far it is from the main line (`History::read_facts`).
    pub(super) fn read_branch_facts(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.history.update(cx, |h, cx| h.read_facts(root, cx));
    }

    /// Reads the working tree's files again when they changed since the last read (`History::read_tree`).
    pub(super) fn read_tree_files(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let (wt, settings) = ((self.wt.count, self.wt.hash), self.settings(cx));
        self.history
            .update(cx, |h, cx| h.read_tree(root, wt, settings, cx));
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

    /// The branches a search lists, by index (`History::listed`).
    pub(super) fn listed_branches<'a>(
        &self,
        cx: &'a App,
    ) -> impl Iterator<Item = (usize, &'a BranchRef)> {
        self.history.read(cx).listed()
    }

    /// Branches' rows in the order the list shows them, its filter typed (`History::rows`).
    pub(super) fn branch_rows(&self, cx: &App) -> Vec<BranchRow> {
        self.history.read(cx).rows(&self.bfilter)
    }

    /// "4 local · 6 on origin": the count after Branches' name in the bar.
    pub(super) fn branches_count(&self, cx: &App) -> String {
        self.history.read(cx).count_line()
    }

    /// The row Branches selects when nothing is (`History::first_row`).
    pub(super) fn first_branch_row(&self, cx: &App) -> Option<BranchRow> {
        self.history.read(cx).first_row(self.wt.count)
    }

    /// A root list's row picked, by a click or ↑ ↓.
    pub(super) fn pick_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        self.nav.pick_request = Some(iid);
        self.requests.read(cx).rows.reveal.set(true);
        self.read_preview(cx);
        cx.notify();
    }

    pub(super) fn toggle_remote(&mut self, remote: String, cx: &mut Context<Self>) {
        self.history.update(cx, |h, _| h.toggle_remote(remote));
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
        let picked = self
            .nav
            .pick_branch
            .filter(|r| rows.contains(r))
            .or_else(|| self.first_branch_row(cx).filter(|r| rows.contains(r)))
            .or(rows.first().copied());
        let acts = self.history_acts(cx);
        let (list, preview) = {
            let beside = |ix: usize| self.beside_branch(ix, cx);
            let look = ListLook {
                room: self.widths.room("branches"),
                changes: self.wt.count,
                filter: self.bfilter.trim(),
                picked,
                beside: &beside,
            };
            let h = self.history.read(cx);
            (
                branches::rows(h, &rows, &look, &acts),
                branches::preview(h, &look, &acts),
            )
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

    /// What the Branches list shows of branch `ix` that other features know: your review of its tip, its request,
    /// its card.
    fn beside_branch(&self, ix: usize, cx: &App) -> Beside {
        let Some(b) = self.history.read(cx).branches.get(ix) else {
            return Beside::default();
        };
        let review = self.review.read(cx);
        Beside {
            part: review.head_part(b.tip),
            reviewed: review.head_reviewed(b.tip),
            request: self
                .request_of_branch(ix, cx)
                .map(|m| (m.iid, m.title.clone())),
            card: self.branch_card(ix, cx).map(|card| Box::new(card) as Card),
        }
    }

    /// A row of Branches clicked: a second click on the one picked opens it, as one click does
    /// narrow (#80); else it is picked, the working tree's files read.
    pub(super) fn pick_branch_row(
        &mut self,
        row: BranchRow,
        clicks: usize,
        cx: &mut Context<Self>,
    ) {
        let open = self.nav.pick_branch == Some(row) && clicks > 1;
        if open || self.compact() {
            return self.open_branch_row(row, cx);
        }
        self.zone = super::zones::Zone::Branches;
        self.nav.pick_branch = Some(row);
        if row == BranchRow::WorkingTree {
            self.read_tree_files(cx);
        }
        cx.notify();
    }

    /// Opens a row of Branches: the working tree, or the branch.
    pub(super) fn open_branch_row(&mut self, row: BranchRow, cx: &mut Context<Self>) {
        self.field = None;
        match row {
            BranchRow::WorkingTree => self.select_working_tree(cx),
            BranchRow::Branch(ix) => self.open_branch(ix, None, cx),
        }
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
