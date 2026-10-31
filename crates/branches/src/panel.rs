//! The side panel of a branch's or a request's page (#49): one island of sections one under the other — Versions,
//! Commits, Changes — each folding to a header that still says what is in it (a count, the first item, how far the
//! review is). An open Commits or Changes takes the room left; Versions takes what its rows need. The tab says which
//! sections there are, folded or open, and what each holds; a click on a header asks it `Act::Fold`.

use crate::history::{Act, Acts, History, Selection};
use crate::side::{search_chip, version_rows, working_chip};
use artoo_core::git::WebRemote;
use artoo_ui_kit::hover::cut;
use artoo_ui_kit::motion::{Folds, folding};
use artoo_ui_kit::room::{Heights, Widths};
use artoo_ui_kit::{icons, island, motion, px, theme};
use gpui::{
    AnyElement, ClickEvent, Context, Div, FontWeight, MouseButton, MouseDownEvent, SharedString,
    Stateful, div, prelude::*,
};

/// The height of a section's header, folded or open.
pub const HEADER: f32 = 34.;

/// A section of the panel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fold {
    Versions = 0,
    Commits = 1,
    Changes = 2,
}

/// A section's header: which it is, its words, folded or open.
pub struct Head {
    pub fold: Fold,
    pub title: String,
    /// What it holds, said while it is folded.
    pub summary: Option<String>,
    /// At its right: its count and controls.
    pub end: Vec<AnyElement>,
    /// Folded, or it does not open.
    pub folded: bool,
    /// Whether it opens at all (one file's Changes does not).
    pub opens: bool,
}

/// A section's header: the fold chevron, its title, what it holds when folded, and on the right its count and
/// controls.
pub fn head(h: Head, acts: &Acts) -> Stateful<Div> {
    let id = match h.fold {
        Fold::Versions => "fold-versions",
        Fold::Commits => "fold-commits",
        Fold::Changes => "fold-changes",
    };
    let (fold, folded, acts) = (h.fold, h.folded, acts.clone());
    div()
        .id(id)
        .flex_none()
        .h(px(HEADER))
        .pl(px(8.))
        .pr(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .when(h.opens, |s| {
            s.cursor_pointer().on_click(
                // A double click folds or opens it once, as one click does.
                move |e: &ClickEvent, _, cx| {
                    if e.click_count() < 2 {
                        acts(Act::Fold(fold), cx)
                    }
                },
            )
        })
        // The chevron turns as it folds (#56, 1).
        .child(motion::toward(
            icons::icon("chevron").size(px(14.)).text_color(if h.opens {
                theme::faint()
            } else {
                theme::panel()
            }),
            (id, 1usize),
            !folded,
            false,
            motion::FAST,
            |chevron, v| {
                chevron.with_transformation(gpui::Transformation::rotate(gpui::radians(
                    -std::f32::consts::FRAC_PI_2 * (1. - v),
                )))
            },
        ))
        .child(
            div()
                .flex_none()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::muted())
                .child(h.title.to_uppercase()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(12.))
                .text_color(theme::muted())
                .children(
                    h.summary
                        .filter(|_| folded)
                        .map(|said| cut("summary", said)),
                ),
        )
        .children(h.end)
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
            fold: Fold::Versions,
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
            .child(head(look, acts))
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
            fold: Fold::Commits,
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
            .child(head(header, &acts))
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
            fold: Fold::Commits,
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
                .child(head(header, &acts))
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

/// The panel at `width`, its sections one under the other, outlined while one of its lists has the keyboard;
/// with none of them open (`open_lists`) they sit at the top.
pub fn panel(width: f32, keyboard: bool, sections: Vec<AnyElement>, open_lists: usize) -> Div {
    island()
        .w(px(width))
        .flex_none()
        .h_full()
        .border_2()
        .border_color(if keyboard {
            theme::splitter()
        } else {
            theme::island_border()
        })
        .children(sections)
        // Everything folded: the sections sit at the top.
        .when(open_lists == 0, |s| s.child(div().flex_1()))
}

/// The rule between two sections.
pub fn rule() -> Div {
    div().flex_none().h(px(1.)).bg(theme::island_border())
}

/// A section that shares the panel's height with the other open one: its share grows from nothing (its header
/// alone) as it opens, and back as it folds (#56, 1).
pub fn open_share(section: Div, id: &'static str, open: bool) -> impl IntoElement {
    motion::toward(section, id, open, false, motion::BASE, |s, v| {
        s.flex_grow(v)
            .flex_shrink(1.)
            .flex_basis(px(0.))
            .min_h(px(motion::mix(HEADER, 120., v)))
    })
}

/// A section's count, at the right of its header.
pub fn count(n: String) -> AnyElement {
    div()
        .flex_none()
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(SharedString::from(n))
        .into_any_element()
}
