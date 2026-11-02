//! A branch's or a request's side panel (#49), a view (`Panel`): the history's sections — Versions, Commits and a
//! comparison's commits, each folding to a header that still says what is in it (a count, the first item) — and the
//! review's Changes under them, in the kit's frame (`artoo_ui_kit::panel`). Which sections there are the route on
//! screen says (`History::panel_shows`); the history keeps how its sections are folded (`storage::PANEL`); which
//! list has the keyboard it asks the `Frame` as it draws.

use crate::history::{Act, Acts, History, Selection};
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

/// A section of the panel the history's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    Versions,
    Commits,
}

impl Fold {
    /// Its header's id.
    pub fn id(self) -> &'static str {
        match self {
            Fold::Versions => "fold-versions",
            Fold::Commits => "fold-commits",
        }
    }

    /// Its key in `storage::PANEL`.
    fn key(self) -> &'static str {
        match self {
            Fold::Versions => "fold_versions",
            Fold::Commits => "fold_commits",
        }
    }
}

/// Section `fold`'s header (`artoo_ui_kit::panel::head`), a click on it asking `Act::Fold`.
pub fn head(fold: Fold, h: Head, acts: &Acts) -> Stateful<Div> {
    let acts = acts.clone();
    panel::head(fold.id(), h, move |cx| acts(Act::Fold(fold), cx))
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
            storage::value(storage::PANEL, fold.key(), storage::LAYOUT).is_some_and(|v| v == "true")
        };
        Self {
            versions: folded(Fold::Versions),
            commits: folded(Fold::Commits),
            ..Default::default()
        }
    }

    /// Whether `fold` is folded.
    pub fn folded(&self, fold: Fold) -> bool {
        match fold {
            Fold::Versions => self.versions,
            Fold::Commits => self.commits,
        }
    }

    /// Whether `fold` moves now: folded or opened a moment ago.
    fn moving(&self, fold: Fold) -> bool {
        self.motion.recent(fold.id())
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
            sections.push(self.history.read(cx).versions_section(&acts));
        }

        if shows.commits {
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
        if let Some((changes, open)) = changes {
            if open {
                open_lists += 1;
            }
            sections.push(rule().into_any_element());
            sections.push(changes);
        }

        panel::panel(self.width, zone.is_some(), sections, open_lists)
    }
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
        // Tabbed: a request's page, a branch's off the main line.
        let tabbed = request || branch.as_deref().is_some_and(|b| self.tabbed(b));
        // A commit of the main line is its page's own level: the panel stays as on its branch.
        let commit_level = [
            RequestCommitRoute::KIND,
            RequestCommitsRoute::KIND,
            BranchCommitsRoute::KIND,
        ]
        .contains(&kind)
            || (tabbed && kind == BranchCommitRoute::KIND);
        let on_branch = tabbed && branch.is_some();
        let loose = kind == CompareRoute::KIND || kind == WorkingTreeRoute::KIND;
        let compare = route.and_then(CompareRoute::from_route).is_some_and(|c| {
            self.compare.as_ref().is_some_and(|shown| {
                let ((base, a), (head, b), since) = &shown.of;
                shown.list.as_ref().is_some_and(|l| !l.is_empty())
                    && (base, head, since) == (&c.base, &c.head, &c.since)
                    && (a.to_string(), b.to_string()) == (c.base_commit, c.head_commit)
            })
        });
        Shows {
            versions: self.versions.len() > 1 && !commit_level && !on_branch && !loose,
            commits: !tabbed && !loose,
            compare,
        }
    }

    /// Section `fold` folded or opened, as the user leaves it; its rows fold with it (#56, 1).
    pub fn fold(&mut self, fold: Fold, cx: &mut Context<Self>) {
        let folded = match fold {
            Fold::Versions => &mut self.sections.versions,
            Fold::Commits => &mut self.sections.commits,
        };
        *folded = !*folded;
        storage::set_value(storage::PANEL, fold.key(), &folded.to_string());
        self.sections.motion.mark(fold.id());
        motion::redraw_after(cx);
        cx.notify();
    }

    /// The Commits section's rows are drawn: open, or folding shut a moment ago.
    pub fn commits_drawn(&self) -> bool {
        !self.sections.commits || self.sections.moving(Fold::Commits)
    }

    /// The Versions section: its rows under its header, folded or open; those from `new_from` on arrived since the
    /// user last looked.
    pub fn versions_section(&self, acts: &Acts) -> AnyElement {
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
                            .child(version_rows(self, self.new_from, acts)),
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
        let mut end: Vec<AnyElement> = Vec::new();
        end.extend(
            working_chip(
                look.working,
                self.selection == Selection::WorkingTree,
                &acts,
            )
            .map(IntoElement::into_any_element),
        );
        end.push(search_chip(self.find.text.is_some(), &acts).into_any_element());
        // Older history not read yet: "1000+" (#25).
        let more = if self.older.is_some() { "+" } else { "" };
        end.push(count(format!("{}{more}", self.commits.len())));
        let folded = self.sections.commits;
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
                s.children(look.field)
                    .children(self.pill(&acts))
                    .child(self.render_commits(
                        self.widths.clone(),
                        look.web.clone(),
                        acts.clone(),
                        cx,
                    ))
            });
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
