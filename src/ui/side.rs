//! The side panel of the redesign (Design `design/shell/`, #49): one island of sections one under
//! the other — Versions, Commits, Changes — each folding to a header that still says what is in it
//! (a count, the first item, how far the review is). An open Commits or Changes takes the room left;
//! Versions takes what its rows need. Each section's feature keeps its fold; ⌥⌘2 folds Changes.

use super::nav::Page;
use super::zones::Zone;
use super::{Selection, Workspace};
use artoo_branches::history::{Act, Acts};
use artoo_branches::panel::{CommitsLook, Fold};
use artoo_review::changes_view::SectionLook;
use artoo_ui_kit::panel::{panel, rule};
use gpui::{AnyElement, App, Context, prelude::*};
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

    /// The panel shows the versions: a request's page with more than one, not on its commits nor
    /// a branch's.
    fn side_versions(&self, cx: &App) -> bool {
        let page = self.nav_page();
        let tabbed = !self.item_sections(cx).is_empty();
        // A commit of the main line is its page's own level: the panel stays as on its branch.
        let commit_level = matches!(
            page,
            Some(Page::RequestCommit(..) | Page::RequestCommits(..) | Page::BranchCommits(..))
        ) || (tabbed && matches!(page, Some(Page::BranchCommit(..))));
        let branch = tabbed
            && matches!(
                page,
                Some(Page::Branch(_) | Page::BranchCommit(..) | Page::BranchCommits(..))
            );
        self.history.read(cx).versions.len() > 1 && !commit_level && !branch && !self.loose_page()
    }

    /// The panel shows the commits: on a page without a Commits tab (the main line), not on a
    /// comparison or the working tree.
    pub(super) fn side_commits(&self, cx: &App) -> bool {
        self.item_sections(cx).is_empty() && !self.loose_page()
    }

    /// A comparison or the working tree: no branch's page, so the versions and commits loaded
    /// are whatever branch or request was opened before it, and its panel holds only its changes
    /// (#147).
    fn loose_page(&self) -> bool {
        matches!(self.nav_page(), Some(Page::Compare(..) | Page::WorkingTree))
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
                if self.side_versions(cx) {
                    icons.push(("layers", "Versions", None));
                }
                if (self.side_commits(cx) && !self.history.read(cx).commits.is_empty())
                    || self.shown_compare(cx).is_some()
                {
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

    /// The side panel; None when it is hidden (⌘J, zen).
    pub(super) fn render_side(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        // Hidden a moment ago (⌘J, zen), it is drawn on its way out (#56, 4).
        if !self.layout.show_sidebar && !self.motion.recent("side") && !self.side_drawer() {
            return None;
        }
        let keyboard = matches!(self.zone, Zone::Commits | Zone::Files);
        let mut open_lists = 0;
        let mut sections: Vec<AnyElement> = Vec::new();

        // A request's or a branch's commits are its Commits tab, a commit of it a level there: on
        // its pages the panel holds the changes — a request's with its versions — and on a commit
        // only the commit's (#73, #74).
        if self.side_versions(cx) {
            sections.push(
                self.history
                    .read(cx)
                    .versions_section(&self.history_acts(cx)),
            );
        }

        if self.side_commits(cx) {
            let folded = self.history.read(cx).sections.commits;
            let rows = self.history.read(cx).commits_drawn();
            let look = CommitsLook {
                tag: self.zone_tag(Zone::Commits).into_owned(),
                working: self.history.read(cx).wt.count,
                field: (rows && self.history.read(cx).find.text.is_some())
                    .then(|| self.render_find(cx).into_any_element()),
                web: self.web.clone(),
            };
            if !sections.is_empty() {
                sections.push(rule().into_any_element());
            }
            if !folded {
                open_lists += 1;
            }
            let acts = self.history_acts(cx);
            sections.push(
                self.history
                    .update(cx, |h, cx| h.commits_section(look, acts, cx)),
            );
        }

        // A comparison's own commits over its changes, as tall as their rows (#21).
        if self.shown_compare(cx).is_some() {
            if !sections.is_empty() {
                sections.push(rule().into_any_element());
            }
            let (web, acts) = (self.web.clone(), self.history_acts(cx));
            let tag = self.zone_tag(Zone::Commits);
            sections.extend(
                self.history
                    .update(cx, |h, cx| h.compare_section(&tag, web, acts, cx)),
            );
        }

        let look = SectionLook {
            tag: self.zone_tag(Zone::Files).into_owned(),
            list: self.changes_look(cx),
        };
        let host = Rc::new(super::TabHost(cx.entity().downgrade()));
        if let Some((changes, open)) = self
            .review
            .update(cx, |rv, cx| rv.changes_section(look, host, cx))
        {
            if open {
                open_lists += 1;
            }
            sections.push(rule().into_any_element());
            sections.push(changes);
        }

        Some(panel(self.layout.sidebar, keyboard, sections, open_lists))
    }
}
