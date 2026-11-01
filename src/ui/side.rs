//! The side panel of the redesign (Design `design/shell/`, #49): one island of sections one under
//! the other — Versions, Commits, Changes — each folding to a header that still says what is in it
//! (a count, the first item, how far the review is). An open Commits or Changes takes the room left;
//! Versions takes what its rows need. Each section's feature keeps its fold; ⌥⌘2 folds Changes.

use super::nav::Page;
use super::zones::Zone;
use super::{Selection, Workspace};
use artoo_branches::history::{Act, Acts};
use artoo_branches::panel::{Fold, Shows};
use gpui::{App, Context};
use std::rc::Rc;

impl Workspace {
    /// How the history's views ask the tab.
    pub(super) fn history_acts(&self, cx: &Context<Self>) -> Acts {
        let this = cx.weak_entity();
        Rc::new(move |act, cx| {
            this.update(cx, |this, cx| match act {
                Act::Version { ix, more } => {
                    let (from, to) = this.history.read(cx).versions_clicked(ix, more);
                    this.pick(Selection::Versions { from, to }, cx)
                }
                Act::Commit(ix) => {
                    this.close_drawer(cx);
                    this.pick(Selection::Commit(ix), cx)
                }
                Act::CommitMenu { ix, at } => this.commit_context(ix, at, cx),
                Act::PickCompare { ix, span } => {
                    // Picked in the drawer, it goes and the diff is on screen (#80).
                    this.close_drawer(cx);
                    this.pick_compare_commit(ix, span, cx)
                }
                Act::WholeCompare => {
                    this.unpick_compare(cx);
                }
                Act::WorkingTree => this.open_working_tree(cx),
                Act::FindCommits => {
                    if this.history.read(cx).sections.commits {
                        this.history.update(cx, |h, cx| h.fold(Fold::Commits, cx));
                    }
                    this.start_find(cx)
                }
                Act::FindField => this.start_find(cx),
                Act::Fold(fold) => this.history.update(cx, |h, cx| h.fold(fold, cx)),
                Act::CommitsZone => this.set_zone(Zone::Commits, cx),
                Act::BranchMenu { ix, at } => this.branch_context(ix, at, cx),
                Act::PickRow { row, clicks } => this.pick_branch_row(row, clicks, cx),
                Act::OpenRow(row) => this.open_branch_row(row, cx),
                Act::ShowRequest(iid) => this.open_request(iid, cx),
                Act::ToggleRemote(remote) => this.toggle_remote(remote, cx),
                Act::OnlyUnstaged(on) => this.set_only_unstaged(on, cx),
                Act::Compare {
                    base,
                    head,
                    since,
                    request,
                } => this.run_compare(base, head, since, request, cx),
                Act::Versions { from, to } => this.pick(Selection::Versions { from, to }, cx),
                Act::VersionTab(tab) => {
                    this.history.update(cx, |h, _| h.version_tab = tab);
                    cx.notify();
                }
                Act::ShowNew => this.apply_pending(cx),
                Act::Message => {
                    this.history
                        .update(cx, |h, _| h.message_open = !h.message_open);
                    cx.notify();
                }
                Act::Copy(text) => this.run_act(super::menu::Act::Copy(text), cx),
                Act::OpenUrl(url) => this.run_act(super::menu::Act::OpenUrl(url), cx),
            })
            .ok();
        })
    }

    /// ⌥⌘2, Changes' header: the review folds or opens it, kept as the user leaves it outside zen.
    /// A folded section gives the keyboard back to the diff as the tab is drawn.
    pub(super) fn fold_changes(&mut self, cx: &mut Context<Self>) {
        let (open, keep) = (!self.review.read(cx).changes_open, self.zen.is_none());
        self.review
            .update(cx, |rv, cx| rv.open_changes(open, keep, true, cx));
    }

    /// What the panel holds on the page on screen (`History::panel_shows`).
    pub(super) fn panel_shows(&self, cx: &App) -> Shows {
        let route = self.nav_page().map(Page::route);
        self.history.read(cx).panel_shows(route.as_ref())
    }

    /// The panel's sections on this page as icons, where it folds into the menu's column (#80):
    /// its icon, its name, and the zone it gives the keyboard to.
    pub(super) fn side_icons(&self, cx: &App) -> Vec<(&'static str, &'static str, Option<Zone>)> {
        if self.settings_open || self.zen.is_some() {
            return Vec::new();
        }
        match self.nav.space {
            super::spaces::Space::Code if self.root.is_some() => {
                vec![("folder", "Files", None)]
            }
            super::spaces::Space::Branches | super::spaces::Space::Requests
                if self.nav_page().is_some()
                    && !matches!(
                        self.page_tab(cx),
                        Some(super::nav::Section::Overview | super::nav::Section::Commits)
                    ) =>
            {
                let mut icons = Vec::new();
                let shows = self.panel_shows(cx);
                if shows.versions {
                    icons.push(("layers", "Versions", None));
                }
                if (shows.commits && !self.history.read(cx).commits.is_empty()) || shows.compare {
                    icons.push(("commit", "Commits", Some(Zone::Commits)));
                }
                if self
                    .review
                    .read(cx)
                    .diff
                    .as_ref()
                    .is_some_and(|d| !d.files.is_empty())
                {
                    icons.push(("files", "Changes", Some(Zone::Files)));
                }
                icons
            }
            _ => Vec::new(),
        }
    }
}
