//! A commit's Overview, its first tab beside Changes: its message as a page — the title with Open on the host beside
//! it, who and when, the message with its issue keys as links —, and what it changed with a way to the Changes.
//! The tab keeps the side panel beside it, so the commits stay in view.

use crate::head::is_trailer;
use crate::history::{Act, Acts, History};
use artoo_branches_api::BranchCommitRoute;
use artoo_core::route::Place;
use artoo_review_api::Review;
use artoo_shell_api::Frame;
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::{discussion, icons, island, px, theme};
use gpui::{
    App, ClickEvent, Context, Entity, FontWeight, IntoElement, Render, Window, div, prelude::*,
};
use std::rc::{Rc, Weak};

/// The Overview of a branch's commit, a view the tab puts under the branch's bar.
pub struct CommitOverview {
    history: Entity<History>,
    frame: Weak<dyn Frame>,
}

/// The tab's view of it, made with its parts, among its services.
pub fn view(
    history: Entity<History>,
    frame: &Rc<dyn Frame>,
    cx: &mut App,
) -> Entity<CommitOverview> {
    let frame = Rc::downgrade(frame);
    cx.new(|_| CommitOverview { history, frame })
}

impl Render for CommitOverview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(frame) = self.frame.upgrade() else {
            return div().flex_1();
        };
        let services = frame.services();
        let Some(commit) = frame
            .route(cx)
            .and_then(|r| BranchCommitRoute::from_route(&r))
            .and_then(|r| git2::Oid::from_str(&r.commit).ok())
            .and_then(|id| {
                self.history
                    .read(cx)
                    .commits
                    .iter()
                    .find(|c| c.id == id)
                    .cloned()
            })
        else {
            return div().flex_1();
        };
        let acts = services.get::<Acts>();
        let files = services
            .get::<Rc<dyn Review>>()
            .and_then(|r| r.shown(cx))
            .map(|d| {
                d.files.iter().fold((d.files.len(), 0, 0), |(n, a, r), f| {
                    (n, a + f.added, r + f.removed)
                })
            });
        let web = frame.places(cx).web;
        let body = commit
            .message
            .split_once('\n')
            .map(|(_, body)| {
                body.lines()
                    .filter(|l| !is_trailer(l))
                    .collect::<Vec<_>>()
                    .join("\n")
                    .trim()
                    .to_owned()
            })
            .filter(|b| !b.is_empty());
        let selecting = frame.selecting(cx);
        let cx: &App = cx;
        let text = |key: String, text: &str| {
            artoo_ui_kit::markdown::body(text, false, key, None, selecting.clone(), cx)
        };
        let open = web.map(|web| {
            let url = web.commit(&commit.id.to_string());
            let name = web.name().to_owned();
            let acts = acts.clone();
            div()
                .id("open-web")
                .flex_none()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(artoo_ui_kit::ROW_RADIUS))
                .border_1()
                .border_color(theme::island_border())
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover()))
                .child(
                    icons::icon("external")
                        .size(px(14.))
                        .text_color(theme::muted()),
                )
                .tip(format!("Open in {name}"))
                .on_click(move |_: &ClickEvent, _, cx| {
                    if let Some(acts) = &acts {
                        acts(Act::OpenUrl(url.clone()), cx)
                    }
                })
        });
        let page = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_4()
            .p(px(24.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text(
                                format!("commit-summary-{}", commit.id),
                                &commit.summary,
                            )),
                    )
                    .children(open),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .child(discussion::avatar(&commit.author, false, 18.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(commit.author.clone()),
                    )
                    .child(
                        div()
                            .id("commit-time")
                            .flex_none()
                            .text_color(theme::muted())
                            .child(format::short_ago(commit.time))
                            .tip(hover::full_date(commit.time)),
                    ),
            )
            .children(body.map(|b| {
                div()
                    .text_size(px(13.))
                    .child(text(format!("commit-body-{}", commit.id), &b))
            }))
            .children(files.map(|(n, added, removed)| {
                let acts = acts.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pt_4()
                    .border_t_1()
                    .border_color(theme::island_border())
                    .text_size(px(12.))
                    .child(div().text_color(theme::muted()).child(plural(n, "file")))
                    .child(div().text_color(theme::added()).child(format!("+{added}")))
                    .child(
                        div()
                            .text_color(theme::removed())
                            .child(format!("−{removed}")),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("commit-to-changes")
                            .text_color(theme::accent())
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::text()))
                            .child("Changes ›")
                            .on_click(move |_: &ClickEvent, _, cx| {
                                if let Some(acts) = &acts {
                                    acts(Act::CommitChanges, cx)
                                }
                            }),
                    )
            }));
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(island().flex_1().min_h_0().child(page))
    }
}
