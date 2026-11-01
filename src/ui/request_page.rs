//! A merge request's page (#73): the section tabs of its bar — Overview, Commits, Changes — and
//! what the first two show; Changes is the diff, with one line of identity over it. Overview holds
//! what the Details tool window held (#8), as a page (`artoo_requests::page` draws it and the
//! line). A request you have never been inside opens on it, later ones on Changes
//! (`nav::first_section`). Commits lists the request's commits with the version each came with
//! (`artoo_branches::commits_page`); one opens as a level under it.

use super::nav::{Item, Page, Section};
use super::{Selection, Workspace, island, plural, px, theme};
use crate::mr::Mr;
use artoo_requests::page::{Facts, OverviewLook};
use gpui::{AnyElement, App, ClickEvent, Context, Div, SharedString, div, prelude::*};

impl Workspace {
    /// The request of the page on screen, once it is the one loaded.
    fn page_request<'a>(&self, cx: &'a App) -> Option<&'a Mr> {
        let Some(Item::Request(iid)) = self.nav_page()?.item() else {
            return None;
        };
        self.requests
            .read(cx)
            .find(iid)
            .filter(|_| self.requests.read(cx).current == Some(iid))
    }

    /// An item's Overview or Commits under its bar, the tool window beside it: a request's, or a
    /// branch's Commits (#74).
    pub(super) fn render_tab_page(&self, tab: Section, cx: &mut Context<Self>) -> Div {
        let body = match (self.page_request(cx).cloned(), tab) {
            _ if matches!(self.nav_page(), Some(Page::Branch(_))) => {
                let line = self
                    .piece_named("branches.line", cx)
                    .unwrap_or_else(|| div().into_any_element());
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(super::gap()))
                    .child(line)
                    .child(self.commits_page(false, cx))
                    .into_any_element()
            }
            (None, _) => island()
                .flex_1()
                .bg(theme::editor())
                .px(px(28.))
                .py(px(26.))
                .child(super::roots::skeleton("request-loading", 4))
                .into_any_element(),
            (Some(m), Section::Overview) => self.render_overview(&m, cx),
            (Some(m), _) => self.render_commits_tab(&m, cx),
        };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .child(body)
            .children(self.render_tool(cx))
    }

    /// Overview (`Requests::render_overview`).
    fn render_overview(&self, m: &Mr, cx: &mut Context<Self>) -> AnyElement {
        let head = git2::Oid::from_str(&m.head).ok();
        let reviewed = self
            .review
            .read(cx)
            .review_progress()
            .map(|(done, total)| (done as u32, total as u32))
            .or_else(|| head.and_then(|h| self.review.read(cx).head_part(h)))
            .map_or_else(
                || "—".to_owned(),
                |(done, total)| format!("{done} of {}", plural(total as usize, "file")),
            );
        let latest = self.history.read(cx).versions.last().map(|v| v.number);
        let versions = match (self.marked_version(m.iid, cx), latest) {
            (Some(at), Some(latest)) if at < latest => format!("v{at} → v{latest}"),
            (_, Some(latest)) => format!("v{latest}"),
            _ => "—".to_owned(),
        };
        let look = OverviewLook {
            title: self.selectable(
                format!("title-{}", m.iid),
                &m.title,
                cx.entity().downgrade(),
                cx,
            ),
            base: self.web.as_ref().map(|w| w.base.clone()),
            selecting: Some(self.selecting(cx.entity().downgrade())),
            facts: Facts {
                commits: self.history.read(cx).commits.len(),
                reviewed,
                versions,
            },
        };
        self.requests.read(cx).render_overview(m, look, cx)
    }

    /// A request's Commits: one line of identity, then its commits, each with the version it came
    /// with.
    fn render_commits_tab(&self, m: &Mr, cx: &mut Context<Self>) -> AnyElement {
        let line = self.request_line(m, cx);
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(super::gap()))
            .child(line)
            .child(self.commits_page(true, cx))
            .into_any_element()
    }

    /// An item's commits (`History::commits_page`), with the version each came with when
    /// `versions`.
    fn commits_page(&self, versions: bool, cx: &mut Context<Self>) -> AnyElement {
        let acts = self.history_acts(cx);
        self.history
            .update(cx, |h, cx| h.commits_page(versions, &acts, cx))
    }

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
                self.history.read(cx).page.as_ref().and_then(|f| f.files),
            ),
            Item::Compare(..) | Item::WorkingTree => return None,
        };
        let count = |section: Section| match section {
            Section::Overview => None,
            Section::Commits => loaded.then_some(self.history.read(cx).commits.len()),
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
        self.history.update(cx, |h, cx| {
            h.step_pick(down);
            cx.notify()
        });
    }

    /// ↵ in a request's Commits: the commit picked opens as a level under it, or the run picked
    /// as one diff.
    pub(super) fn open_commit_pick(&mut self, cx: &mut Context<Self>) {
        if let Some((newest, oldest)) = self.history.read(cx).picked_run() {
            return self.pick(Selection::Commits { newest, oldest }, cx);
        }
        if let Some(ix) = self.history.read(cx).picked() {
            self.pick(Selection::Commit(ix), cx);
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
        let Some(at) = self
            .history
            .read(cx)
            .commits
            .iter()
            .position(|c| c.id == id)
        else {
            return false;
        };
        let to = if forward {
            at.checked_sub(1)
        } else {
            (at + 1 < self.history.read(cx).commits.len()).then_some(at + 1)
        };
        if let Some(to) = to {
            self.pick(Selection::Commit(to), cx);
        }
        true
    }

    fn request_line(&self, m: &Mr, cx: &mut Context<Self>) -> Div {
        let title = self.selectable(
            format!("line-title-{}", m.iid),
            &m.title,
            cx.entity().downgrade(),
            cx,
        );
        let acts = self.request_acts(cx);
        self.requests.read(cx).request_line(m, title, acts)
    }
}
