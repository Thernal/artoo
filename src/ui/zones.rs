//! Keyboard focus zones: the branches, the commits, the files, the diff and the tool window on the
//! right. One of them has the
//! keyboard at a time (its island has an outline), Tab and ⇧Tab move between them, ↑ ↓ ← → work in
//! the one that has it, and ↵ goes one step in.

use super::px;
use super::{Workspace, theme};
use artoo_review_api::ItemKey;
use artoo_shell_api::ListKey;
use gpui::{App, Context, Rgba};

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Zone {
    Branches,
    #[default]
    Commits,
    Files,
    Diff,
    /// The tool window (`super::tools`).
    Tool,
}

/// How far a key scrolls the diff.
const LINE: f32 = 60.;
const PAGE: f32 = 520.;

impl Workspace {
    /// The outline colour of a zone's island.
    pub(super) fn zone_border(&self, zone: Zone) -> Rgba {
        if self.zone == zone {
            theme::splitter()
        } else {
            theme::island_border()
        }
    }

    /// " · ⌃2": the island that has the keyboard says so in its label, and how to get back to it.
    pub(super) fn zone_tag(&self, zone: Zone) -> std::borrow::Cow<'static, str> {
        if self.zone != zone {
            return "".into();
        }
        super::keys::text(match zone {
            Zone::Branches => " · ⌃1",
            Zone::Commits => " · ⌃2",
            Zone::Files => " · ⌃3",
            Zone::Diff => " · ⌃4",
            Zone::Tool => "",
        })
    }

    pub(super) fn set_zone(&mut self, zone: Zone, cx: &mut Context<Self>) {
        if self.zone != zone {
            self.zone = zone;
            cx.notify();
        }
    }

    /// The zones that exist right now, in Tab order.
    fn zones(&self, cx: &App) -> Vec<Zone> {
        let mut out = Vec::new();
        // A page with no side panel and no diff: a space's list, the Inbox, Code (#71).
        let item = matches!(
            self.nav.space,
            super::spaces::Space::Branches | super::spaces::Space::Requests
        ) && !self.at_root_list();
        // A request's Overview and Commits: no side panel and no diff (#73).
        let tab_page = matches!(
            self.page_tab(cx),
            Some(super::nav::Section::Overview | super::nav::Section::Commits)
        );
        if !item || tab_page {
            if self.at_root_list() {
                out.push(Zone::Branches);
            }
            if self.tool_shown(cx) && !self.settings_open {
                out.push(Zone::Tool);
            }
            return out;
        }
        // A tabbed item's commits are its Commits tab, not a section of the panel (#73, #74).
        if self.side_open()
            && !self.branches().is_some_and(|b| b.commits_folded(cx))
            && (self.panel_shows(cx).commits || self.shown_compare(cx))
        {
            out.push(Zone::Commits);
        }
        if self.shown_diff(cx).is_some() && !self.settings_open {
            if self.files_zone(cx) {
                out.push(Zone::Files);
            }
            out.push(Zone::Diff);
        }
        if self.tool_shown(cx) && !self.settings_open {
            out.push(Zone::Tool);
        }
        out
    }

    pub(super) fn step_zone(&mut self, forward: bool, cx: &mut Context<Self>) {
        let zones = self.zones(cx);
        if zones.is_empty() {
            return;
        }
        let at = zones.iter().position(|z| *z == self.zone);
        let next = match (at, forward) {
            (Some(i), true) => (i + 1) % zones.len(),
            (Some(i), false) => (i + zones.len() - 1) % zones.len(),
            (None, _) => 0,
        };
        self.set_zone(zones[next], cx);
    }

    /// ↑ and ↓ in the zone that has the keyboard.
    pub(super) fn zone_vertical(&mut self, down: bool, cx: &mut Context<Self>) {
        // The New merge request card's From or Into list (#159).
        if self.step_branch_list(down, cx) {
            return;
        }
        // The Inbox has the window: j k walk its list.
        if self.inbox && self.zone != Zone::Tool {
            return self.inbox_step(down, cx);
        }
        if self.page_tab(cx) == Some(super::nav::Section::Commits) && self.zone != Zone::Tool {
            return self.step_commit_pick(down, cx);
        }
        match self.zone {
            Zone::Commits if self.shown_compare(cx) => {
                return self.step_compare_pick(down, cx);
            }
            Zone::Commits => self.step_commit_row(down, cx),
            Zone::Branches => self.step_pick(down, cx),
            Zone::Files => self.step_file_item(down, cx),
            Zone::Diff => self
                .diff_list(cx)
                .scroll_by(px(if down { LINE } else { -LINE })),
            Zone::Tool => {
                if let Some(id) = self.tool() {
                    let key = if down { ListKey::Down } else { ListKey::Up };
                    self.part_list_key(id, key, cx);
                }
            }
        }
        cx.notify();
    }

    /// ← and → : the neighbouring file, or in the file tree open and close folders.
    pub(super) fn zone_horizontal(&mut self, forward: bool, cx: &mut Context<Self>) {
        match self.zone {
            Zone::Files
                if !self.view_options(cx).list_files
                    && self.rv().path_filter(cx).trim().is_empty() =>
            {
                self.tree_horizontal(forward, cx)
            }
            Zone::Branches if forward => self.set_zone(Zone::Commits, cx),
            Zone::Tool if !forward => self.set_zone(Zone::Diff, cx),
            Zone::Tool => {}
            Zone::Diff if !forward => self.set_zone(
                if self.files_zone(cx) {
                    Zone::Files
                } else {
                    Zone::Commits
                },
                cx,
            ),
            _ => self.step_file(forward, cx),
        }
    }

    pub(super) fn step_file(&mut self, forward: bool, cx: &mut Context<Self>) {
        let order = self.rv().file_order(cx);
        let Some(at) = order.iter().position(|&ix| ix == self.diff_file(cx)) else {
            return;
        };
        let to = if forward {
            (at + 1 < order.len()).then(|| at + 1)
        } else {
            at.checked_sub(1)
        };
        if let Some(to) = to {
            self.select_file(order[to], cx);
        }
    }

    /// ↵: one step in.
    pub(super) fn activate(&mut self, cx: &mut Context<Self>) {
        // Set up's Next, or Done on its last part (#143).
        if self.setup.is_some() {
            return self.setup_next(cx);
        }
        if self.inbox && self.zone != Zone::Tool {
            return self.inbox_open(cx);
        }
        // A space's list: what is selected in it opens (#71).
        if self.at_root_list() && self.zone == Zone::Branches {
            return self.open_pick(cx);
        }
        if self.page_tab(cx) == Some(super::nav::Section::Commits) && self.zone != Zone::Tool {
            return self.open_commit_pick(cx);
        }
        match self.zone {
            Zone::Tool => {
                if let Some(id) = self.tool() {
                    self.part_list_key(id, ListKey::Enter, cx);
                }
            }
            Zone::Branches => {}
            Zone::Commits => {
                if self.shown_diff(cx).is_some() {
                    self.set_zone(
                        if self.files_zone(cx) {
                            Zone::Files
                        } else {
                            Zone::Diff
                        },
                        cx,
                    );
                }
            }
            Zone::Files => {
                if let Some(dir) = self.rv().fcursor(cx) {
                    self.toggle_dir(&dir, cx);
                } else {
                    self.set_zone(Zone::Diff, cx);
                }
            }
            Zone::Diff => {
                // A folded generated file opens on ↵.
                if let Some(path) = (self.shown_diff(cx).as_ref())
                    .and_then(|d| d.files.get(self.diff_file(cx)))
                    .filter(|f| self.rv().generated_folded(f, cx))
                    .map(|f| f.path().to_owned())
                {
                    self.rv().unfold_generated(path, cx);
                    cx.notify();
                }
            }
        }
    }

    /// PageUp, PageDown, Space: a screen of the diff.
    pub(super) fn page(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.scroll_page(down, cx) {
            return;
        }
        self.diff_list(cx)
            .scroll_by(px(if down { PAGE } else { -PAGE }));
        cx.notify();
    }

    /// Home and End: the top or the bottom of the diff.
    pub(super) fn diff_edge(&mut self, end: bool, cx: &mut Context<Self>) {
        if end {
            self.diff_list(cx).scroll_to_end();
        } else {
            self.diff_list(cx).scroll_to(gpui::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
        }
        cx.notify();
    }

    /// The working tree entry above the commits counts as the row before the first commit.
    fn step_commit_row(&mut self, down: bool, cx: &mut Context<Self>) {
        match self.branches().and_then(|b| b.step_row(down, cx)) {
            Some(super::Selection::Commit(ix)) => self.pick(super::Selection::Commit(ix), cx),
            Some(super::Selection::WorkingTree) => self.open_working_tree(cx),
            _ => {}
        }
    }

    /// ↑ ↓ in a space's list: the row selected, nothing opened (#71); the picked request's
    /// preview is read (#72).
    pub(super) fn step_pick(&mut self, down: bool, cx: &mut Context<Self>) {
        if !self.at_root_list() {
            return;
        }
        let step = |at: Option<usize>, n: usize| match (at, down) {
            (Some(i), true) => Some((i + 1).min(n.saturating_sub(1))),
            (Some(i), false) => Some(i.saturating_sub(1)),
            (None, _) => (n > 0).then_some(0),
        };
        if self.nav.space == super::spaces::Space::Requests {
            let shown: Vec<u64> = self.listed_requests(cx).iter().map(|m| m.iid).collect();
            let at = self
                .requests_api()
                .and_then(|r| r.picked_row(cx))
                .and_then(|iid| shown.iter().position(|i| *i == iid));
            if let Some(iid) = step(at, shown.len()).map(|i| shown[i]) {
                self.pick_request(iid, cx);
            }
        } else {
            let rows = self.branch_rows(cx);
            let at = self
                .picked_branch_row(cx)
                .and_then(|r| rows.iter().position(|x| *x == r));
            let picked = step(at, rows.len()).map(|i| rows[i]);
            if let Some(branches) = self.branches() {
                branches.pick_row(picked, cx);
            }
        }
        cx.notify();
    }

    /// ↵ in a space's list: the row selected opens.
    pub(super) fn open_pick(&mut self, cx: &mut Context<Self>) {
        if self.nav.space == super::spaces::Space::Requests {
            let shown: Vec<u64> = self.listed_requests(cx).iter().map(|m| m.iid).collect();
            let pick = self
                .requests_api()
                .and_then(|r| r.picked_row(cx))
                .filter(|iid| shown.contains(iid));
            if let Some(iid) = pick.or(shown.first().copied()) {
                self.open_picked_request(iid, cx);
            }
        } else {
            let rows = self.branch_rows(cx);
            let pick = self.picked_branch_row(cx).filter(|r| rows.contains(r));
            if let Some(row) = pick.or(rows.first().copied()) {
                self.open_branch_row(row, cx);
            }
        }
    }

    /// ↑ ↓ over what the files island shows, folders included.
    fn step_file_item(&mut self, down: bool, cx: &mut Context<Self>) {
        let items = self.rv().item_keys(cx);
        let here = match &self.rv().fcursor(cx) {
            Some(dir) => items
                .iter()
                .position(|k| matches!(k, ItemKey::Dir(d) if d == dir)),
            None => items
                .iter()
                .position(|k| matches!(k, ItemKey::File(ix) if *ix == self.diff_file(cx))),
        };
        let to = match (here, down) {
            (Some(i), true) => (i + 1 < items.len()).then(|| i + 1),
            (Some(i), false) => i.checked_sub(1),
            (None, _) => (!items.is_empty()).then_some(0),
        };
        match to.map(|i| items[i].clone()) {
            Some(ItemKey::File(ix)) => {
                self.rv().set_fcursor(None, cx);
                self.select_file(ix, cx);
            }
            Some(ItemKey::Dir(path)) => {
                self.rv().set_fcursor(Some(path), cx);
                cx.notify();
            }
            None => {}
        }
    }

    /// In the tree: → opens a folder or goes into the diff, ← closes a folder or goes up to it.
    fn tree_horizontal(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(dir) = self.rv().fcursor(cx) {
            let open = !self.rv().dir_folded(&dir, cx);
            match (forward, open) {
                (true, false) | (false, true) => self.toggle_dir(&dir, cx),
                (true, true) => self.step_file_item(true, cx),
                (false, false) => {}
            }
            return;
        }
        if forward {
            self.set_zone(Zone::Diff, cx);
            return;
        }
        // ← on a file: the folder it is in, or its group (#22).
        if let Some(path) = self.rv().file_key(self.diff_file(cx), cx) {
            let parent = (self.rv().item_keys(cx))
                .into_iter()
                .filter_map(|k| match k {
                    ItemKey::Dir(d) if path.starts_with(&format!("{d}/")) => Some(d),
                    _ => None,
                })
                .next_back();
            if parent.is_some() {
                self.rv().set_fcursor(parent, cx);
                cx.notify();
            }
        }
    }
}
