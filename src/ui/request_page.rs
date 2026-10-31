//! A merge request's page (#73): the section tabs of its bar — Overview, Commits, Changes — and
//! what the first two show; Changes is the diff, with one line of identity over it. Overview holds
//! what the Details tool window held (#8), as a page (`artoo_requests::page` draws it and the
//! line). A request you have never been inside opens on it, later ones on Changes
//! (`nav::first_section`). Commits lists the request's commits with the version each came with;
//! one opens as a level under it.

use super::hover::CutInTip;
use super::nav::{Item, Page, Section};
use super::{Selection, Workspace, format, icons, island, plural, px, theme};
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
                    .child(self.commits_list(None, cx))
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
        let tips: Vec<(usize, git2::Oid)> = self
            .history
            .read(cx)
            .versions
            .iter()
            .map(|v| (v.number, v.tip))
            .collect();
        let ids: Vec<git2::Oid> = self.history.read(cx).commits.iter().map(|c| c.id).collect();
        let versions = commit_versions(&ids, &tips);
        let line = self.request_line(m, cx);
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(super::gap()))
            .child(line)
            .child(self.commits_list(Some(versions), cx))
            .into_any_element()
    }

    /// An item's commits, newest first, with the version each came with when it has versions;
    /// ↑ ↓ pick one, ↵ or a second click opens it as a level.
    fn commits_list(
        &self,
        versions: Option<Vec<Option<usize>>>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use super::hover::Hover;
        let versions = versions.unwrap_or_else(|| vec![None; self.history.read(cx).commits.len()]);
        let picked = self.picked_commit(cx);
        // Shift picks a run of them, to review as one diff.
        let run = self.picked_run(cx);
        // A commit's anatomy (#79): its sha and title always; when, and who when there are
        // several, while the list has 260 px; the icon and its version under 360 px leave first.
        // The tooltip holds them all.
        let room = self.widths.room("commits");
        let several = self.history.read(cx).commits.first().is_some_and(|first| {
            self.history
                .read(cx)
                .commits
                .iter()
                .any(|c| c.author != first.author)
        });
        let rows: Vec<_> = self
            .history
            .read(cx)
            .commits
            .iter()
            .enumerate()
            .map(|(ix, c)| {
                let selected = match run {
                    Some((newest, oldest)) => (newest..=oldest).contains(&ix),
                    None => picked == Some(ix),
                };
                let mut tip = vec![
                    c.summary.clone(),
                    format!(
                        "{} · {} · {}",
                        format::short(c.id),
                        c.author,
                        super::hover::full_date(c.time)
                    ),
                ];
                if let Some(v) = versions[ix] {
                    tip.push(format!("came with v{v}"));
                }
                super::row(("request-commit", ix), selected)
                    .h(px(36.))
                    .gap(px(10.))
                    .px(px(10.))
                    .when(room.third(), |s| {
                        s.child(
                            icons::icon("commit")
                                .size(px(14.))
                                .text_color(theme::faint()),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme::code_font())
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .child(format::short(c.id)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .cut_in_tip()
                            .child(c.summary.clone()),
                    )
                    .children((several && room.second()).then(|| {
                        div()
                            .flex_none()
                            .max_w(px(140.))
                            .cut_in_tip()
                            .text_size(px(11.))
                            .text_color(theme::muted())
                            .child(c.author.clone())
                    }))
                    .children(versions[ix].filter(|_| room.third()).map(|v| {
                        div()
                            .flex_none()
                            .px(px(6.))
                            .rounded_full()
                            .border_1()
                            .border_color(theme::accent())
                            .text_size(px(11.))
                            .text_color(theme::accent())
                            .child(format!("v{v}"))
                    }))
                    .when(room.second(), |s| {
                        s.child(
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(format::ago(c.time)),
                        )
                    })
                    .tip(tip.join("\n"))
                    // Its SHA, its message, its link (#162).
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this, e: &gpui::MouseDownEvent, _, cx| {
                            this.commit_context(ix, e.position, cx)
                        }),
                    )
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        let id = this.history.read(cx).commits.get(ix).map(|c| c.id);
                        if event.modifiers().shift {
                            // The run from the one picked to this one.
                            if this.nav.pick_commit.is_none() {
                                this.nav.pick_commit = this.picked_commit(cx).and_then(|at| {
                                    this.history.read(cx).commits.get(at).map(|c| c.id)
                                });
                            }
                            this.nav.pick_to = id;
                            return cx.notify();
                        }
                        if this.picked_commit(cx) == Some(ix) && event.click_count() > 1 {
                            return this.pick(Selection::Commit(ix), cx);
                        }
                        this.nav.pick_commit = id;
                        this.nav.pick_to = None;
                        cx.notify();
                    }))
            })
            .collect();
        island()
            .flex_1()
            .min_h_0()
            .bg(theme::editor())
            .child(
                div()
                    .id("request-commits")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(8.))
                    .py(px(8.))
                    .child(self.widths.measure("commits"))
                    .child(
                        div()
                            .id("commits-count")
                            .px(px(10.))
                            .pb(px(6.))
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(match run {
                                Some((newest, oldest)) => format!(
                                    "{} of {} picked · ↵ reviews them as one",
                                    oldest - newest + 1,
                                    plural(self.history.read(cx).commits.len(), "commit")
                                ),
                                None => plural(self.history.read(cx).commits.len(), "commit"),
                            })
                            .tip("Newest first · ↵ opens one, esc comes back here · shift and a click picks a run of them to review as one"),
                    )
                    .children(rows),
            )
            .into_any_element()
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

    /// The commit Commits has picked: the one esc came back from, else the newest.
    fn picked_commit(&self, cx: &App) -> Option<usize> {
        let at = self.nav.pick_commit.and_then(|id| {
            self.history
                .read(cx)
                .commits
                .iter()
                .position(|c| c.id == id)
        });
        at.or((!self.history.read(cx).commits.is_empty()).then_some(0))
    }

    /// The run of commits picked with shift: the newest's index and the oldest's.
    fn picked_run(&self, cx: &App) -> Option<(usize, usize)> {
        let at = |id: git2::Oid| {
            self.history
                .read(cx)
                .commits
                .iter()
                .position(|c| c.id == id)
        };
        let a = at(self.nav.pick_commit?)?;
        let b = at(self.nav.pick_to?)?;
        (a != b).then_some((a.min(b), a.max(b)))
    }

    /// ↑ ↓ in a request's Commits: another commit picked, nothing opened.
    pub(super) fn step_commit_pick(&mut self, down: bool, cx: &mut Context<Self>) {
        let n = self.history.read(cx).commits.len();
        let to = match (self.picked_commit(cx), down) {
            (Some(at), true) => (at + 1).min(n.saturating_sub(1)),
            (Some(at), false) => at.saturating_sub(1),
            (None, _) => return,
        };
        self.nav.pick_commit = self.history.read(cx).commits.get(to).map(|c| c.id);
        self.nav.pick_to = None;
        cx.notify();
    }

    /// ↵ in a request's Commits: the commit picked opens as a level under it, or the run picked
    /// as one diff.
    pub(super) fn open_commit_pick(&mut self, cx: &mut Context<Self>) {
        if let Some((newest, oldest)) = self.picked_run(cx) {
            return self.pick(Selection::Commits { newest, oldest }, cx);
        }
        if let Some(ix) = self.picked_commit(cx) {
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

/// Each commit's version, the commits newest first: the version whose tip it is, else the one of
/// the commit above it — it came with that push. None above the first tip found (a push rewrote
/// what the versions knew).
fn commit_versions(commits: &[git2::Oid], tips: &[(usize, git2::Oid)]) -> Vec<Option<usize>> {
    let mut at = None;
    commits
        .iter()
        .map(|id| {
            // The first push it was the tip of: when it came.
            if let Some(&(number, _)) = tips.iter().find(|(_, tip)| tip == id) {
                at = Some(number);
            }
            at
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::commit_versions;
    use git2::Oid;

    fn id(n: u8) -> Oid {
        Oid::from_bytes(&[n; 20]).unwrap()
    }

    #[test]
    fn a_commit_is_of_the_push_it_came_with() {
        // Newest first: 3 is v3's tip, 2 v2's, 1 came with v2 too (v1's tip was 0).
        let commits = [id(3), id(2), id(1), id(0)];
        let tips = [(1, id(0)), (2, id(2)), (3, id(3))];
        assert_eq!(
            commit_versions(&commits, &tips),
            [Some(3), Some(2), Some(2), Some(1)]
        );
        // A push rewrote them all: none of the tips is among them but the last.
        let rewritten = [id(9), id(8), id(7)];
        assert_eq!(
            commit_versions(&rewritten, &[(1, id(0)), (2, id(9))]),
            [Some(2), Some(2), Some(2)]
        );
        assert_eq!(commit_versions(&[id(1)], &[]), [None]);
    }
}
