//! The side panel of the redesign (Design `design/shell/`, #49): one island of sections one under
//! the other — Versions, Commits, Changes — each folding to a header that still says what is in it
//! (a count, the first item, how far the review is). An open Commits or Changes takes the room left;
//! Versions takes what its rows need. Folds are kept in `layout.txt`; ⌥⌘2 folds Changes.

use super::nav::Page;
use super::zones::Zone;
use super::{Workspace, chip, plural};
use artoo_branches::history::{Act, Acts};
use artoo_branches::panel::{
    CommitsLook, Fold, Folding, Head, count, head, open_share, panel, rule,
};
use gpui::{AnyElement, App, ClickEvent, Context, MouseButton, div, prelude::*};
use std::rc::Rc;

impl Workspace {
    /// How the history's views ask the tab.
    pub(super) fn history_acts(&self, cx: &Context<Self>) -> Acts {
        let this = cx.weak_entity();
        Rc::new(move |act, cx| {
            this.update(cx, |this, cx| match act {
                Act::Version { ix, more } => {
                    let (from, to) = this.history.read(cx).versions_clicked(ix, more);
                    this.select_versions(from, to, cx)
                }
                Act::Commit(ix) => {
                    this.close_drawer(cx);
                    this.select_commit(ix, cx)
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
                Act::WorkingTree => this.select_working_tree(cx),
                Act::FindCommits => {
                    if this.layout.fold_commits {
                        this.fold_section(1, cx);
                    }
                    this.start_find(cx)
                }
                Act::FindField => this.start_find(cx),
                Act::Fold(fold) => this.fold_section(fold as u8, cx),
                Act::CommitsZone => this.set_zone(Zone::Commits, cx),
                Act::BranchMenu { ix, at } => this.branch_context(ix, at, cx),
                Act::PickRow { row, clicks } => this.pick_branch_row(row, clicks, cx),
                Act::OpenRow(row) => this.open_branch_row(row, cx),
                Act::ShowRequest(iid) => this.select_request(iid, cx),
                Act::ToggleRemote(remote) => this.toggle_remote(remote, cx),
                Act::OnlyUnstaged(on) => this.set_only_unstaged(on, cx),
                Act::Compare {
                    base,
                    head,
                    since,
                    request,
                } => this.run_compare(base, head, since, request, cx),
                Act::Versions { from, to } => this.select_versions(from, to, cx),
                Act::VersionTab(tab) => {
                    this.history.update(cx, |h, _| h.version_tab = tab);
                    cx.notify();
                }
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

    fn section_folded(&self, section: Fold) -> bool {
        match section {
            Fold::Versions => self.layout.fold_versions,
            Fold::Commits => self.layout.fold_commits,
            Fold::Changes => !self.layout.show_files,
        }
    }

    pub(super) fn fold_section(&mut self, section: u8, cx: &mut Context<Self>) {
        // Its rows stay drawn while it folds (#56, 1).
        self.mark_motion(&format!("fold-{section}"), cx);
        let fold = match section {
            0 => &mut self.layout.fold_versions,
            1 => &mut self.layout.fold_commits,
            _ => {
                self.layout.show_files = !self.layout.show_files;
                return self.after_fold(cx);
            }
        };
        *fold = !*fold;
        self.after_fold(cx);
    }

    fn after_fold(&mut self, cx: &mut Context<Self>) {
        self.save_layout();
        // A folded section gives the keyboard back to the diff.
        let gone = match self.zone {
            Zone::Commits => self.layout.fold_commits,
            Zone::Files => !self.files_zone(cx),
            _ => false,
        };
        if gone && self.review.read(cx).diff.is_some() {
            self.set_zone(Zone::Diff, cx);
        }
        cx.notify();
    }

    /// A section's header (`artoo_branches::panel::head`); `opens` is false for a section that cannot open.
    fn section_header(
        &self,
        section: Fold,
        title: String,
        summary: Option<String>,
        end: Vec<AnyElement>,
        opens: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let look = Head {
            fold: section,
            title,
            summary,
            end,
            folded: self.section_folded(section) || !opens,
            opens,
        };
        head(look, &self.history_acts(cx))
    }

    /// The panel shows the versions: a request's page with more than one, not on its commits nor
    /// a branch's.
    fn side_versions(&self, cx: &App) -> bool {
        let page = self.nav_page();
        let tabbed = !self.item_sections(cx).is_empty();
        let commit_level = matches!(
            page,
            Some(
                Page::RequestCommit(..)
                    | Page::BranchCommit(..)
                    | Page::RequestCommits(..)
                    | Page::BranchCommits(..)
            )
        );
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
            let fold = Folding {
                folds: &self.folds,
                heights: &self.heights,
                moving: self.motion.recent("fold-0"),
            };
            sections.push(self.history.read(cx).versions_section(
                self.section_folded(Fold::Versions),
                fold,
                self.watch.new_from,
                &self.history_acts(cx),
            ));
        }

        if self.side_commits(cx) {
            let folded = self.section_folded(Fold::Commits);
            let rows = !folded || self.motion.recent("fold-1");
            let look = CommitsLook {
                folded,
                rows,
                tag: self.zone_tag(Zone::Commits).into_owned(),
                working: self.wt.count,
                field: (rows && self.history.read(cx).find.text.is_some())
                    .then(|| self.render_find(cx).into_any_element()),
                pill: rows
                    .then(|| self.render_pill(cx).map(IntoElement::into_any_element))
                    .flatten(),
                widths: self.widths.clone(),
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
            let (folded, tag) = (
                self.section_folded(Fold::Commits),
                self.zone_tag(Zone::Commits),
            );
            let fold = Folding {
                folds: &self.folds,
                heights: &self.heights,
                moving: self.motion.recent("fold-1"),
            };
            sections.extend(self.history.update(cx, |h, cx| {
                h.compare_section(folded, fold, &tag, web, acts, cx)
            }));
        }

        if let Some(diff) = self
            .review
            .read(cx)
            .diff
            .as_ref()
            .filter(|d| !d.files.is_empty())
        {
            let one = diff.files.len() == 1;
            let folded = one || self.section_folded(Fold::Changes);
            let summary = if one {
                Some(diff.files[0].path().to_owned())
            } else {
                Some(match self.review.read(cx).review_progress() {
                    Some((done, total)) => format!("{done} / {total} reviewed"),
                    None => plural(diff.file_count(), "file"),
                })
            };
            let list = self.review.read(cx).options.list_files;
            let mut end: Vec<AnyElement> = Vec::new();
            if !folded {
                end.push(
                    artoo_ui_kit::controls::group()
                        // A chip is not the heading: a click on it does not fold the section.
                        .child(chip("files-tree", "Tree", !list).on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.set_file_view(false, cx)
                            },
                        )))
                        .child(chip("files-list", "List", list).on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.set_file_view(true, cx)
                            },
                        )))
                        .into_any_element(),
                );
            }
            end.push(count(diff.file_count().to_string()));
            let done = self.review.read(cx).all_reviewed();
            if !folded {
                open_lists += 1;
            }
            sections.push(rule().into_any_element());
            let rows = !folded || (!one && self.motion.recent("fold-2"));
            let changes = div()
                .flex()
                .flex_col()
                .overflow_hidden()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        if this.files_zone(cx) {
                            this.set_zone(Zone::Files, cx)
                        }
                    }),
                )
                .child(self.section_header(
                    Fold::Changes,
                    format!(
                        "Changes{}{}",
                        if done { " ✓" } else { "" },
                        self.zone_tag(Zone::Files)
                    ),
                    summary,
                    end,
                    !one,
                    cx,
                ))
                .when(rows, |s| s.child(self.render_changes(cx)));
            sections.push(open_share(changes, "changes-open", !folded).into_any_element());
        }

        Some(panel(self.layout.sidebar, keyboard, sections, open_lists))
    }
}
