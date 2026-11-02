//! A merge request's page (#73): the section tabs of its bar — Overview, Commits, Changes — and
//! where the first two go; Changes is the diff, with one line of identity over it. Overview holds
//! what the Details tool window held (#8), as a page, and Commits the request's commits with the
//! version each came with, the requests' view (`artoo_requests::tabs`). A request you have never
//! been inside opens on Overview, later ones on Changes (`nav::first_section`). A commit opens as
//! a level under Commits.

use super::nav::{Item, Page, Section};
use super::{Selection, Workspace};
use gpui::{ClickEvent, Context, SharedString, prelude::*};

impl Workspace {
    /// The section tabs in an item's bar with their counts — a request's Overview, Commits and
    /// Changes, a branch's Commits and Changes (#74); ⌃⇥ walks them.
    pub(super) fn section_tabs(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let at = self.page_section(cx)?;
        let (loaded, files) = match self.nav_page()?.item()? {
            Item::Request(iid) => (
                self.requests.read(cx).current == Some(iid),
                self.requests
                    .read(cx)
                    .files
                    .filter(|(of, _)| *of == iid)
                    .map(|(_, n)| n),
            ),
            // Tabbed only while it is the branch loaded.
            Item::Branch(_) => (
                true,
                self.branches().and_then(|b| b.branch_files(cx)),
            ),
            Item::Compare(..) | Item::WorkingTree => return None,
        };
        let count = |section: Section| match section {
            Section::Overview => None,
            Section::Commits => loaded.then_some(self.commit_count(cx)),
            Section::Changes => files,
        };
        Some(self.item_sections(cx).iter().copied().fold(
            artoo_ui_kit::controls::group(),
            |group, section| {
                let label = match count(section) {
                    Some(n) => format!("{} {n}", section.label()),
                    None => section.label().to_owned(),
                };
                group.child(
                    artoo_ui_kit::controls::chip(
                        SharedString::from(format!("section-{}", section.label())),
                        label,
                        section == at,
                    )
                    .on_click(cx.listener(
                        move |this, _: &ClickEvent, _, cx| this.show_section(section, cx),
                    )),
                )
            },
        ))
    }

    /// ↑ ↓ in a request's Commits: another commit picked, nothing opened.
    pub(super) fn step_commit_pick(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(branches) = self.branches() {
            branches.step_pick(down, cx);
        }
    }

    /// ↵ in a request's Commits: the commit picked opens as a level under it, or the run picked
    /// as one diff.
    pub(super) fn open_commit_pick(&mut self, cx: &mut Context<Self>) {
        if let Some(picked) = self.branches().and_then(|b| b.picked(cx)) {
            self.pick(picked, cx);
        }
    }

    /// [ and ] on a request's or a branch's commit: the commit before or after it in time (the list is newest
    /// first). Whether the page was one.
    pub(super) fn step_item_commit(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let (Some(Page::RequestCommit(_, id)) | Some(Page::BranchCommit(_, id))) =
            self.nav_page().cloned()
        else {
            return false;
        };
        let Some(at) = self.commit_at(id, cx)
        else {
            return false;
        };
        let to = if forward {
            at.checked_sub(1)
        } else {
            (at + 1 < self.commit_count(cx)).then_some(at + 1)
        };
        if let Some(to) = to {
            self.pick(Selection::Commit(to), cx);
        }
        true
    }
}
