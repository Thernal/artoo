//! A branch's or a request's side panel (#49), a view (`Panel`): the history's sections — Versions, Commits and a
//! comparison's commits, each folding to a header that still says what is in it (a count, the first item) — and the
//! review's Changes under them, in the kit's frame (`artoo_ui_kit::panel`). Which sections there are the route on
//! screen says (`History::panel_shows`); the history keeps how its sections are folded (`storage::PANEL`); which
//! list has the keyboard it asks the `Frame` as it draws.

use crate::history::{Act, Acts, History, Selection};

/// The most commits the Working tree's Commits shows before it scrolls (#209).
const WORK_ROWS: usize = 6;
/// A commit's row in the side panel.
pub(crate) const COMMIT_ROW: f32 = 62.;
use crate::side::{search_chip, version_rows, working_chip};
use artoo_branches_api::{
    BranchCommitRoute, BranchCommitsRoute, BranchRoute, CompareRoute, WorkingTreeRoute,
};
use artoo_core::git::WebRemote;
use artoo_core::route::{Place, Route};
use artoo_core::storage;
use artoo_requests_api::{
    RequestCommitRoute, RequestCommitsRoute, RequestRoute, RequestVersionsRoute,
};
use artoo_shell_api::{Frame, ListZone, input_box};
use artoo_ui_kit::motion::{self, Folds, Marks, folding};
use artoo_ui_kit::panel::{self, Head, count, open_share, rule};
use artoo_ui_kit::px;
use artoo_ui_kit::room::Heights;
use gpui::{
    AnyElement, App, Context, Div, Entity, MouseButton, MouseDownEvent, Render, Stateful, Window,
    div, prelude::*,
};
use std::rc::{Rc, Weak};

pub use artoo_branches_api::Fold;

/// Section `fold`'s header's id.
fn id(fold: Fold) -> &'static str {
    match fold {
        Fold::Versions => "fold-versions",
        Fold::Commits => "fold-commits",
    }
}

/// Section `fold`'s key in `storage::PANEL`.
fn key(fold: Fold) -> &'static str {
    match fold {
        Fold::Versions => "fold_versions",
        Fold::Commits => "fold_commits",
    }
}

/// Section `fold`'s header (`artoo_ui_kit::panel::head`), a click on it asking `Act::Fold`.
pub fn head(fold: Fold, h: Head, acts: &Acts) -> Stateful<Div> {
    let acts = acts.clone();
    panel::head(id(fold), h, move |cx| acts(Act::Fold(fold), cx))
}

/// How the history's sections are folded, as the user left them, and how they fold now: what each fold was last
/// drawn as, its height then, its toggle's mark.
#[derive(Default)]
pub struct Sections {
    /// Versions folded to its header.
    pub versions: bool,
    /// Commits folded to its header.
    pub commits: bool,
    motion: Marks,
    folds: Folds,
    heights: Heights,
}

impl Sections {
    /// As the user left them.
    pub fn load() -> Self {
        let folded = |fold: Fold| {
            storage::value(storage::PANEL, key(fold), storage::LAYOUT).is_some_and(|v| v == "true")
        };
        Self {
            versions: folded(Fold::Versions),
            commits: folded(Fold::Commits),
            ..Default::default()
        }
    }

    /// Whether `fold` moves now: folded or opened a moment ago.
    fn moving(&self, fold: Fold) -> bool {
        self.motion.recent(id(fold))
    }
}

/// What the Commits section needs of the tab besides the history.
pub struct CommitsLook {
    /// The keyboard's mark on its title (` · ⌃2` while its list has it).
    pub tag: String,
    /// The working tree's changes, for its chip.
    pub working: usize,
    /// The search's field, as the tab gives it the keyboard.
    pub field: Option<AnyElement>,
    /// The repository's web address, for a card's link.
    pub web: Option<WebRemote>,
}

pub use artoo_branches_api::PanelShows as Shows;

/// The side panel of the tab's branch's and request's pages, at the width the shell gives it (`Pages::side`).
pub struct Panel {
    history: Entity<History>,
    frame: Weak<dyn Frame>,
    pub width: f32,
}

/// The tab's panel, made with its parts (`install`), among its services.
pub fn view(history: Entity<History>, frame: &Rc<dyn Frame>, cx: &mut App) -> Entity<Panel> {
    let frame = Rc::downgrade(frame);
    cx.new(|_| Panel {
        history,
        frame,
        width: 0.,
    })
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = self.frame.upgrade() else {
            return div();
        };
        let services = frame.services();
        let acts = services.get::<Acts>().unwrap_or_else(|| Rc::new(|_, _| {}));
        let zone = frame.list_zone(cx);
        let tag = |at: ListZone| {
            zone.as_ref()
                .filter(|(z, _)| *z == at)
                .map_or_else(String::new, |(_, tag)| tag.to_string())
        };
        let web = frame.places(cx).web;
        let route = frame.route(cx);
        let shows = self.history.read(cx).panel_shows(route.as_ref());
        let mut open_lists = 0;
        let mut sections: Vec<AnyElement> = Vec::new();

        // A request's or a branch's commits are its Commits tab, a commit of it a level there: on
        // its pages the panel holds the changes — a request's with its versions — and on a commit
        // only the commit's (#73, #74).
        if shows.versions {
            let h = self.history.read(cx);
            // The version you read, tagged (#187).
            let read = (services.get::<Rc<dyn artoo_review_api::Review>>())
                .and_then(|review| h.read_version(&*review, cx));
            sections.push(h.versions_section(read, &acts));
        }

        if shows.commits {
            fill_viewed(&self.history, &services, false, cx);
            let h = self.history.read(cx);
            let field = (h.commits_drawn() && h.find.text.is_some()).then(|| {
                let active = frame.typing(cx) == Some(crate::find::INPUT);
                let h = self.history.read(cx);
                let el = crate::find::field(h, active, self.history.downgrade(), &acts);
                input_box(el, crate::find::INPUT, &frame).into_any_element()
            });
            let h = self.history.read(cx);
            let look = CommitsLook {
                tag: tag(ListZone::Commits),
                working: h.wt.count,
                field,
                web: web.clone(),
            };
            if !sections.is_empty() {
                sections.push(rule().into_any_element());
            }
            if !h.sections.commits {
                open_lists += 1;
            }
            let acts = acts.clone();
            sections.push(
                self.history
                    .update(cx, |h, cx| h.commits_section(look, acts, cx)),
            );
        }

        // A comparison's own commits over its changes, as tall as their rows (#21).
        if shows.compare {
            if !sections.is_empty() {
                sections.push(rule().into_any_element());
            }
            let (tag, acts) = (tag(ListZone::Commits), acts.clone());
            sections.extend(
                self.history
                    .update(cx, |h, cx| h.compare_section(&tag, web, acts, cx)),
            );
        }

        let changes = services
            .get::<Rc<dyn artoo_review_api::Review>>()
            .zip(services.get::<Rc<dyn artoo_review_api::Host>>())
            .and_then(|(review, host)| review.changes_section(tag(ListZone::Files), host, cx));
        if let Some((changes, open)) = changes.filter(|_| !shows.no_changes) {
            if open {
                open_lists += 1;
            }
            sections.push(rule().into_any_element());
            sections.push(changes);
        }

        panel::panel(self.width, zone.is_some(), sections, open_lists)
    }
}

/// The rows above the first in view whose counts are read, and a quarter of all those read.
const AROUND: usize = 20;

/// The commits of `len` whose counts are read with row `top` the first in view: a screen's worth around it.
fn counted(top: usize, len: usize) -> std::ops::Range<usize> {
    let from = top.saturating_sub(AROUND).min(len);
    from..(from + AROUND * 4).min(len)
}

/// How much of each of the history's commits you viewed, asked of the review (`Review` in the tab's `services`) for
/// their rows (#209), read forty at a time: in the side panel's list those in view and a screen's worth around them,
/// however far down it is scrolled; on the Commits page, which draws every row, `all` of them.
pub fn fill_viewed(
    history: &Entity<History>,
    services: &artoo_shell_api::Services,
    all: bool,
    cx: &mut App,
) {
    let Some(review) = services.get::<Rc<dyn artoo_review_api::Review>>() else {
        return;
    };
    let h = history.read(cx);
    // The first row in view: how far the list is scrolled, over the height of a row as it drew them last.
    let top = {
        let scroll = h.commit_scroll.0.borrow();
        let row = (scroll.last_item_size).map_or(0., |s| {
            f32::from(s.contents.height) / h.commits.len().max(1) as f32
        });
        let down = -f32::from(scroll.base_handle.offset().y);
        if row > 0. {
            (down / row).max(0.) as usize
        } else {
            0
        }
    };
    let rows = match all {
        true => 0..h.commits.len(),
        false => counted(top, h.commits.len()),
    };
    let ids: Vec<git2::Oid> = h.commits[rows].iter().map(|c| c.id).collect();
    let viewed = review.commit_viewed(&ids, cx);
    let viewed = (ids.into_iter().zip(viewed))
        .filter_map(|(id, v)| Some((id, v?)))
        .collect();
    history.update(cx, |h, _| h.viewed = viewed);
}

impl History {
    /// What the panel holds on `route`: the versions on a request's page with more than one, not on its commits nor
    /// a branch's; the commits on a page without a Commits tab (the main line), not on a comparison nor the working
    /// tree, which have only their changes (#147); a comparison's own commits once read.
    pub fn panel_shows(&self, route: Option<&Route>) -> Shows {
        let kind = route.map_or("", |r| r.kind.as_str());
        let request = [
            RequestRoute::KIND,
            RequestCommitRoute::KIND,
            RequestCommitsRoute::KIND,
            RequestVersionsRoute::KIND,
        ]
        .contains(&kind);
        let branch = route.and_then(|route| {
            BranchRoute::from_route(route)
                .map(|b| b.refname)
                .or_else(|| BranchCommitRoute::from_route(route).map(|b| b.refname))
                .or_else(|| BranchCommitsRoute::from_route(route).map(|b| b.refname))
        });
        // The Working tree of the branch checked out, or its own pages: its commits over its whole work (#209).
        let working = self.checked_out().is_some()
            && (kind == WorkingTreeRoute::KIND
                || branch
                    .as_deref()
                    .is_some_and(|b| self.shows_working_tree(b)));
        // Tabbed: a request's page, a branch's off the main line.
        let tabbed = request || (!working && branch.as_deref().is_some_and(|b| self.tabbed(b)));
        // A commit of the main line is its page's own level: the panel stays as on its branch.
        let commit_level = [
            RequestCommitRoute::KIND,
            RequestCommitsRoute::KIND,
            BranchCommitsRoute::KIND,
        ]
        .contains(&kind)
            || (tabbed && kind == BranchCommitRoute::KIND);
        let on_branch = tabbed && branch.is_some();
        let loose = kind == CompareRoute::KIND || (kind == WorkingTreeRoute::KIND && !working);
        let compare = route.and_then(CompareRoute::from_route).is_some_and(|c| {
            self.compare.as_ref().is_some_and(|shown| {
                let ((base, a), (head, b), since) = &shown.of;
                shown.list.as_ref().is_some_and(|l| !l.is_empty())
                    && (base, head, since) == (&c.base, &c.head, &c.since)
                    && (a.to_string(), b.to_string()) == (c.base_commit, c.head_commit)
            })
        });
        // The main line's Commits and Changes are tabs (#208): one or the other, the whole column.
        let main = branch.as_deref().is_some_and(|b| self.main_line(b));
        Shows {
            versions: self.versions.len() > 1 && !commit_level && !on_branch && !loose && !working,
            commits: !tabbed && !loose && !(main && self.main_changes),
            compare,
            no_changes: main && !self.main_changes,
        }
    }

    /// Section `fold` folded or opened, as the user leaves it; its rows fold with it (#56, 1).
    pub fn fold(&mut self, fold: Fold, cx: &mut Context<Self>) {
        let folded = match fold {
            Fold::Versions => &mut self.sections.versions,
            Fold::Commits => &mut self.sections.commits,
        };
        *folded = !*folded;
        storage::set_value(storage::PANEL, key(fold), &folded.to_string());
        self.sections.motion.mark(id(fold));
        motion::redraw_after(cx);
        cx.notify();
    }

    /// The Commits section's rows are drawn: open, or folding shut a moment ago.
    pub fn commits_drawn(&self) -> bool {
        !self.sections.commits || self.sections.moving(Fold::Commits)
    }

    /// The Versions section: its rows under its header, folded or open; those from `new_from` on arrived since the
    /// user last looked.
    pub fn versions_section(&self, read: Option<usize>, acts: &Acts) -> AnyElement {
        let folded = self.sections.versions;
        let summary = match self.selection {
            Selection::Versions { from, to } => format!(
                "v{} → v{}",
                self.versions[from].number, self.versions[to].number
            ),
            _ => format!("latest v{}", self.versions[self.versions.len() - 1].number),
        };
        let look = Head {
            title: "Versions".into(),
            summary: Some(summary),
            end: vec![count(self.versions.len().to_string())],
            folded,
            opens: true,
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .child(head(Fold::Versions, look, acts))
            // Opening and folding alike, to its own height and back (#110); the padding inside, so nothing is
            // left of it at the end.
            .children(folding(
                &self.sections.folds,
                &self.sections.heights,
                self.sections.moving(Fold::Versions),
                "versions",
                !folded,
                div()
                    .id("versions-list")
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .pb(px(6.))
                            .child(version_rows(self, self.new_from, read, acts)),
                    ),
            ))
            .into_any_element()
    }

    /// The Commits section of a page without a Commits tab (the main line): the commits' chips and count on its
    /// header, the search's field, the pill and the list under it; a press in it gives its list the keyboard.
    pub fn commits_section(
        &self,
        look: CommitsLook,
        acts: Acts,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let first = match self.selection {
            Selection::Commit(ix) => self.commits.get(ix),
            _ => self.commits.first(),
        };
        let summary = first.map(|c| {
            if self.commits.len() > 1 {
                format!("{} +{}", c.summary, self.commits.len() - 1)
            } else {
                c.summary.clone()
            }
        });
        // The branch checked out: its page is the Working tree already, its commits as tall as their rows (#209).
        let checked_out = self.checked_out().is_some();
        let mut end: Vec<AnyElement> = Vec::new();
        end.extend(
            working_chip(
                look.working,
                self.selection == Selection::WorkingTree,
                &acts,
            )
            .filter(|_| !checked_out)
            .map(IntoElement::into_any_element),
        );
        end.push(search_chip(self.find.text.is_some(), &acts).into_any_element());
        // Older history not read yet: "1000+" (#25).
        let more = if self.older.is_some() { "+" } else { "" };
        end.push(count(format!("{}{more}", self.commits.len())));
        let folded = self.sections.commits;
        // Folded or opened a moment ago: what was not drawn before comes in from the other end, as `folding` does.
        let moving = self.sections.moving(Fold::Commits);
        let header = Head {
            title: format!("Commits{}", look.tag),
            summary,
            end,
            folded,
            opens: true,
        };
        let press = acts.clone();
        let section = div()
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.widths.measure("side-commits"))
            .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, _, cx| {
                press(Act::CommitsZone, cx)
            })
            .child(head(Fold::Commits, header, &acts))
            .when(self.commits_drawn(), |s| {
                // The rows fade as the section folds and opens (#56, 1: every fold moves in size and in alpha).
                let rows = div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .children(look.field)
                    .children(self.pill(&acts))
                    .child(self.render_commits(
                        self.widths.clone(),
                        look.web.clone(),
                        acts.clone(),
                        cx,
                    ));
                s.child(motion::toward(
                    rows,
                    "commits-rows",
                    !folded,
                    moving,
                    motion::BASE,
                    |rows, v| rows.opacity(v),
                ))
            });
        if checked_out {
            // As tall as its rows, and its height moves with the fold as the other sections' shares do.
            let rows = self.commit_rows().clamp(1, WORK_ROWS) as f32;
            let body = rows * COMMIT_ROW + 6.;
            return motion::toward(
                section.flex_none(),
                "commits-tall",
                !folded,
                moving,
                motion::BASE,
                move |s, v| s.h(px(panel::HEADER + body * v)),
            )
            .into_any_element();
        }
        open_share(section, "commits-open", !folded).into_any_element()
    }

    /// A comparison's own commits over its changes, as tall as their rows (#21), folded with Commits or open, `tag`
    /// the keyboard's mark on its title; a press in it gives its list the keyboard. `None` until they are read.
    pub fn compare_section(
        &self,
        tag: &str,
        web: Option<WebRemote>,
        acts: Acts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = self.compare.as_ref()?;
        let n = c.list.as_ref()?.len();
        let folded = self.sections.commits;
        let header = Head {
            title: format!("Commits{tag}"),
            summary: c.summary(),
            end: vec![count(n.to_string())],
            folded,
            opens: true,
        };
        let rows = self.render_compare_commits(web, acts.clone(), cx);
        let press = acts.clone();
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, _, cx| {
                    press(Act::CommitsZone, cx)
                })
                .child(head(Fold::Commits, header, &acts))
                .children(rows.and_then(|rows| {
                    folding(
                        &self.sections.folds,
                        &self.sections.heights,
                        self.sections.moving(Fold::Commits),
                        "compare-commits",
                        !folded,
                        rows,
                    )
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::counted;

    #[test]
    fn the_rows_counted_follow_the_list_however_far_it_scrolls() {
        assert_eq!(counted(0, 500), 0..80);
        assert_eq!(counted(300, 500), 280..360);
        assert_eq!(counted(490, 500), 470..500);
        assert_eq!(counted(0, 12), 0..12);
        assert_eq!(counted(40, 12), 12..12);
    }
}
