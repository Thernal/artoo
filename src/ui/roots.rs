//! The roots of Branches and Merge requests (#72): the list, the row selected previewed beside it,
//! the list's filter in the page bar (⌘F), and the list's states — loading, empty, no token, an
//! error, offline. Merge requests are grouped by what they need from you, each group a band that
//! folds (#81), and Mine counts the discussions waiting for you and lists them first. Branches list the working tree first, then
//! the local branches with how far they are from the main line and their request; a remote's
//! branches fold under its name. Each list's rows and preview are their feature's
//! (`artoo_requests::list`; Branches' whole page, `artoo_branches::page`); the tab draws the
//! islands around them (`list_page`).

use super::lists::Field;
use super::nav::BranchRow;
use super::{Workspace, island, keys, px, theme};
use crate::mr::{Mr, MrState, Thread};
pub(super) use artoo_requests_api::Group;
use artoo_review_api::Seen;
use gpui::{AnyElement, App, ClickEvent, Context, Div, WeakEntity, div, prelude::*};

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

    /// A root's list beside its preview (`Frame::list_page`): the list bordered while it has the
    /// keyboard; its handle widens or narrows it, kept for every list page (the developer,
    /// 2026-10-23), a double click setting it back; narrow, the list alone (#80).
    pub(super) fn list_page(
        &self,
        list: impl IntoElement,
        preview: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .child(
                self.root_list()
                    .border_color(self.zone_border(super::zones::Zone::List))
                    .child(list),
            )
            .children((!self.compact()).then(|| self.handle(super::Split::List, cx)))
            .children((!self.compact()).then(|| {
                island()
                    .flex_1()
                    .min_w_0()
                    .bg(theme::editor())
                    .child(preview)
            }))
            .into_any_element()
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
        if let Some(branches) = self.branches() {
            branches.read_facts(cx);
        }
    }

    /// Reads the working tree's files again when they changed since the last read (`History::read_tree`).
    pub(super) fn read_tree_files(&mut self, cx: &mut Context<Self>) {
        if let Some(branches) = self.branches() {
            branches.read_tree(cx);
        }
    }

    /// The request picked in the list: its pipeline and approvals, once per update of it.
    pub(super) fn read_preview(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self
            .requests_api()
            .and_then(|r| r.picked_row(cx))
            .and_then(|iid| self.find_request(iid, cx))
        else {
            return;
        };
        // Pushed since your review: what changed since the push you viewed.
        let head = git2::Oid::from_str(&m.head).ok();
        let viewed = match self.rv().request_seen(m.iid, head, cx) {
            Seen::Earlier => head.zip(self.rv().reviewed_head(m.iid, head, cx)),
            _ => None,
        };
        if let Some(requests) = self.requests_api() {
            requests.read_preview(&m, viewed, cx);
        }
    }

    // ---- what the lists hold ---------------------------------------------------------------

    /// Request `m`'s group in the list.
    pub(super) fn group_of(&self, m: &Mr, cx: &App) -> Group {
        self.requests_api()
            .map_or(Group::New, |r| r.group_of(m, cx))
    }

    /// The discussions of request `iid` waiting for its author (`waiting`). Read with the
    /// notifications (`notices`), so only for requests read there.
    fn waiting_threads(&self, iid: u64, cx: &App) -> Vec<Thread> {
        self.requests_api()
            .map(|r| r.waiting_threads(iid, cx))
            .unwrap_or_default()
    }

    /// Branches' rows in the order the list shows them, its filter typed (`History::rows`).
    pub(super) fn branch_rows(&self, cx: &App) -> Vec<BranchRow> {
        self.branches()
            .map_or_else(Vec::new, |b| b.rows(&self.bfilter, cx))
    }

    /// "4 local · 6 on origin": the count after Branches' name in the bar.
    pub(super) fn branches_count(&self, cx: &App) -> String {
        self.branches()
            .map(|b| b.count_line(cx))
            .unwrap_or_default()
    }

    /// The row Branches has picked, else the one it picks when nothing is.
    pub(super) fn picked_branch_row(&self, cx: &App) -> Option<BranchRow> {
        self.branches().and_then(|b| b.picked_row(cx))
    }

    /// Merge requests' list with nothing picked: the first request listed is.
    pub(super) fn pick_first_request(&mut self, cx: &mut Context<Self>) {
        if self.requests_api().and_then(|r| r.picked_row(cx)).is_none() {
            let first = self.listed_requests(cx).first().map(|m| m.iid);
            if let Some(requests) = self.requests_api() {
                requests.pick_row(first, cx);
            }
        }
    }

    /// A root list's row picked, by a click or ↑ ↓.
    pub(super) fn pick_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        if let Some(requests) = self.requests_api() {
            requests.pick_row(Some(iid), cx);
            requests.reveal_row(cx);
        }
        self.read_preview(cx);
        cx.notify();
    }

    pub(super) fn toggle_remote(&mut self, remote: String, cx: &mut Context<Self>) {
        if let Some(branches) = self.branches() {
            branches.toggle_remote(remote, cx);
        }
        cx.notify();
    }

    /// Whether group `g`'s rows are folded away; a filter typed opens every group.
    fn folds(&self, g: Group, cx: &App) -> bool {
        let filtering = !self.bfilter.trim().is_empty();
        self.requests_api()
            .is_some_and(|r| r.folds(g, filtering, cx))
    }

    /// The requests the list has a row for, in its order: those of folded groups left out, but
    /// the picked one, which stays under its band.
    pub(super) fn listed_requests(&self, cx: &App) -> Vec<Mr> {
        let past = self
            .requests_api()
            .is_some_and(|r| r.view(cx) != MrState::Opened);
        let picked = self.requests_api().and_then(|r| r.picked_row(cx));
        self.shown_requests(cx)
            .into_iter()
            .filter(|m| past || picked == Some(m.iid) || !self.folds(self.group_of(m, cx), cx))
            .collect()
    }

    /// Whether you have been inside request `iid`: opened here, or files of it marked (#81).
    pub(super) fn opened_before(&self, iid: u64, cx: &App) -> bool {
        let head = self
            .find_request(iid, cx)
            .and_then(|m| git2::Oid::from_str(&m.head).ok());
        self.requests_api().is_some_and(|r| r.row_opened(iid, cx))
            || !matches!(self.rv().request_seen(iid, head, cx), Seen::Never)
    }

    // ---- the pages ------------------------------------------------------------------------

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
            None => self.open_request(iid, cx),
        }
    }

    /// ↵ on Merge requests' root: the picked request opens — for one of yours with discussions
    /// waiting, at the first of them.
    pub(super) fn open_picked_request(&mut self, iid: u64, cx: &mut Context<Self>) {
        let me = self.request_login(cx).unwrap_or_default();
        let mine = self
            .find_request(iid, cx)
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

    /// A row of Branches clicked: a second click on the one picked opens it, as one click does
    /// narrow (#80); else it is picked, the working tree's files read.
    pub(super) fn pick_branch_row(
        &mut self,
        row: BranchRow,
        clicks: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(branches) = self.branches() else {
            return;
        };
        let open = branches.picked_row(cx) == Some(row) && clicks > 1;
        if open || self.compact() {
            return self.open_branch_row(row, cx);
        }
        self.zone = super::zones::Zone::List;
        branches.pick_row(Some(row), cx);
        cx.notify();
    }

    /// Opens a row of Branches: the working tree, or the branch.
    pub(super) fn open_branch_row(&mut self, row: BranchRow, cx: &mut Context<Self>) {
        self.field = None;
        match row {
            BranchRow::WorkingTree => self.open_working_tree(cx),
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

/// What Merge requests' own page asks of the tab (`artoo_requests_api::RootHost`).
pub(super) struct RootHost(pub WeakEntity<Workspace>);

impl artoo_requests_api::RootHost for RootHost {
    fn connect(&self, cx: &mut App) -> AnyElement {
        self.0
            .update(cx, |this, cx| this.render_connect(cx).into_any_element())
            .unwrap_or_else(|_| div().into_any_element())
    }

    fn fold(
        &self,
        key: String,
        moving: &str,
        open: bool,
        row: AnyElement,
        cx: &App,
    ) -> Option<AnyElement> {
        self.0.upgrade()?.read(cx).folding(key, moving, open, row)
    }

    fn rich(&self, cx: &App) -> bool {
        self.0
            .upgrade()
            .is_some_and(|tab| tab.read(cx).settings.rich_comments)
    }

    fn narrow(&self, cx: &App) -> bool {
        self.0.upgrade().is_some_and(|tab| tab.read(cx).compact())
    }

    fn more(&self, cx: &mut App) {
        self.0
            .update(cx, |this, cx| {
                if let Some(scope) = this.requests_api().map(|r| r.scope(cx)) {
                    this.load_past_page(scope, cx)
                }
            })
            .ok();
    }
}
