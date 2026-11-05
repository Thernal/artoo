//! The side panel of the redesign (Design `design/shell/`, #49): one island of sections one under
//! the other — Versions, Commits, Changes — each folding to a header that still says what is in it
//! (a count, the first item, how far the review is). An open Commits or Changes takes the room left;
//! Versions takes what its rows need. Each section's feature keeps its fold; ⌥⌘2 folds Changes.

use super::nav::Page;
use super::zones::Zone;
use super::{Selection, Workspace};
use artoo_branches_api::PanelShows;
use artoo_branches_api::{Act, Acts};
use gpui::{App, Context};
use std::rc::Rc;

impl Workspace {
    /// How the history's views ask the tab: what is the history's own it answers first
    /// (`artoo_branches::history::acts`).
    pub(super) fn history_acts(&self, cx: &Context<Self>) -> Acts {
        let this = cx.weak_entity();
        Rc::new(move |act, cx| {
            this.update(cx, |this, cx| match act {
                Act::Version { .. }
                | Act::FindCommits
                | Act::Fold(_)
                | Act::VersionTab(_)
                | Act::Message => {}
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
                Act::FindField => this.start_find(cx),
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
                Act::ShowNew => this.apply_pending(cx),
                Act::Copy(text) => cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)),
                Act::OpenUrl(url) => cx.open_url(&url),
            })
            .ok();
        })
    }

    /// ⌥⌘2, Changes' header: the review folds or opens it, kept as the user leaves it outside zen.
    /// A folded section gives the keyboard back to the diff as the tab is drawn.
    pub(super) fn fold_changes(&mut self, cx: &mut Context<Self>) {
        let (open, keep) = (!self.rv().changes_open(cx), self.zen.is_none());
        self.rv().open_changes(open, keep, true, cx);
    }

    /// What the panel holds on the page on screen (`Branches::panel_shows`).
    pub(super) fn panel_shows(&self, cx: &App) -> PanelShows {
        let route = self.nav_page().map(Page::route);
        self.branches()
            .map(|b| b.panel_shows(route.as_ref(), cx))
            .unwrap_or_default()
    }

    /// The panel's sections on this page as icons, where it folds into the menu's column (#80):
    /// its icon, its name, and the zone it gives the keyboard to.
    pub(super) fn side_icons(&self, cx: &App) -> Vec<(&'static str, &'static str, Option<Zone>)> {
        if self.settings_open || self.zen.is_some() {
            return Vec::new();
        }
        match self.nav.space {
            super::spaces::CODE if self.root.is_some() => {
                vec![("folder", "Files", None)]
            }
            super::spaces::BRANCHES | super::spaces::REQUESTS
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
                if (shows.commits && self.commit_count(cx) > 0) || shows.compare {
                    icons.push(("commit", "Commits", Some(Zone::Commits)));
                }
                if self
                    .shown_diff(cx)
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
