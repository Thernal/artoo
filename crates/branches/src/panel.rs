//! The history's sections of a branch's or a request's side panel (#49): Versions, Commits and a comparison's
//! commits, each folding to a header that still says what is in it (a count, the first item), drawn in the kit's
//! frame (`artoo_ui_kit::panel`); Changes under them is the review's. The tab says which sections there are, folded
//! or open, and what each holds; a click on a header asks it `Act::Fold`.

use crate::history::{Act, Acts, History, Selection};
use crate::side::{search_chip, version_rows, working_chip};
use artoo_core::git::WebRemote;
use artoo_ui_kit::motion::{Folds, folding};
use artoo_ui_kit::panel::{self, Head, count, open_share};
use artoo_ui_kit::px;
use artoo_ui_kit::room::{Heights, Widths};
use gpui::{AnyElement, Context, Div, MouseButton, MouseDownEvent, Stateful, div, prelude::*};

/// A section of the panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    Versions = 0,
    Commits = 1,
    Changes = 2,
}

impl Fold {
    /// Its header's id.
    pub fn id(self) -> &'static str {
        match self {
            Fold::Versions => "fold-versions",
            Fold::Commits => "fold-commits",
            Fold::Changes => "fold-changes",
        }
    }
}

/// Section `fold`'s header (`artoo_ui_kit::panel::head`), a click on it asking `Act::Fold`.
pub fn head(fold: Fold, h: Head, acts: &Acts) -> Stateful<Div> {
    let acts = acts.clone();
    panel::head(fold.id(), h, move |cx| acts(Act::Fold(fold), cx))
}

/// What a section's rows fold with: what each fold was last drawn as and its height then (the tab's), and
/// whether its fold moves now (its toggle's mark fresh).
pub struct Folding<'a> {
    pub folds: &'a Folds,
    pub heights: &'a Heights,
    pub moving: bool,
}

/// What the Commits section needs of the tab besides the history.
pub struct CommitsLook {
    pub folded: bool,
    /// Its rows drawn: open, or folding shut a moment ago.
    pub rows: bool,
    /// The keyboard's mark on its title (` · ⌃2` while its list has it).
    pub tag: String,
    /// The working tree's changes, for its chip.
    pub working: usize,
    /// The search's field, as the tab gives it the keyboard.
    pub field: Option<AnyElement>,
    /// The new commits' pill.
    pub pill: Option<AnyElement>,
    /// The lists' widths, its rows' room measured as `side-commits`.
    pub widths: Widths,
    /// The repository's web address, for a card's link.
    pub web: Option<WebRemote>,
}

impl History {
    /// The Versions section: its rows under its header, `folded` or open; those from `new_from` on arrived since
    /// the user last looked.
    pub fn versions_section(
        &self,
        folded: bool,
        fold: Folding,
        new_from: Option<usize>,
        acts: &Acts,
    ) -> AnyElement {
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
                fold.folds,
                fold.heights,
                fold.moving,
                "versions",
                !folded,
                div()
                    .id("versions-list")
                    .max_h(px(240.))
                    .overflow_y_scroll()
                    .child(div().pb(px(6.)).child(version_rows(self, new_from, acts))),
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
        let header = Head {
            title: format!("Commits{}", look.tag),
            summary,
            end,
            folded: look.folded,
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
            .when(look.rows, |s| {
                s.children(look.field)
                    .children(look.pill)
                    .child(self.render_commits(
                        look.widths.clone(),
                        look.web.clone(),
                        acts.clone(),
                        cx,
                    ))
            });
        open_share(section, "commits-open", !look.folded).into_any_element()
    }

    /// A comparison's own commits over its changes, as tall as their rows (#21): `folded` or open, `tag` the
    /// keyboard's mark on its title; a press in it gives its list the keyboard. `None` until they are read.
    pub fn compare_section(
        &self,
        folded: bool,
        fold: Folding,
        tag: &str,
        web: Option<WebRemote>,
        acts: Acts,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let c = self.compare.as_ref()?;
        let n = c.list.as_ref()?.len();
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
                        fold.folds,
                        fold.heights,
                        fold.moving,
                        "compare-commits",
                        !folded,
                        rows,
                    )
                }))
                .into_any_element(),
        )
    }
}
