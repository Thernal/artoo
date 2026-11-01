//! The history's sections of a branch's or a request's side panel (#49): Versions, Commits and a comparison's
//! commits, each folding to a header that still says what is in it (a count, the first item), drawn in the kit's
//! frame (`artoo_ui_kit::panel`); Changes under them is the review's. The history keeps how its sections are
//! folded (`storage::PANEL`); the tab says which there are and what each holds.

use crate::history::{Act, Acts, History, Selection};
use crate::side::{search_chip, version_rows, working_chip};
use artoo_core::git::WebRemote;
use artoo_core::storage;
use artoo_ui_kit::motion::{self, Folds, Marks, folding};
use artoo_ui_kit::panel::{self, Head, count, open_share};
use artoo_ui_kit::px;
use artoo_ui_kit::room::{Heights, Widths};
use gpui::{AnyElement, Context, Div, MouseButton, MouseDownEvent, Stateful, div, prelude::*};

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
    /// The lists' widths, its rows' room measured as `side-commits`.
    pub widths: Widths,
    /// The repository's web address, for a card's link.
    pub web: Option<WebRemote>,
}

impl History {
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
            .child(look.widths.measure("side-commits"))
            .on_mouse_down(MouseButton::Left, move |_: &MouseDownEvent, _, cx| {
                press(Act::CommitsZone, cx)
            })
            .child(head(Fold::Commits, header, &acts))
            .when(self.commits_drawn(), |s| {
                s.children(look.field)
                    .children(self.pill(&acts))
                    .child(self.render_commits(
                        look.widths.clone(),
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
