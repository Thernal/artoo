//! A merge request's page, the requests' side of it (#73): Overview — the title with its author and branches on one
//! line, the facts in a row, what is in the way of merging without a click (#190, Design L4), the description beside
//! a column of who reviews, its labels, its milestone and its discussions — and the request's identity in words for a
//! long hover. Commits and Changes have no line of their own over them: the title is in the page bar (#188).

use crate::bar::pipeline_word;
use crate::merge::{Fix, Stand};
use crate::requests::Requests;
use crate::waiting::FADED;
use artoo_hosts_api::{Details, Mr, MrState};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover;
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{discussion, island, px, theme, words};
use gpui::{AnyElement, App, ClickEvent, Div, FontWeight, Rgba, div, prelude::*};
use std::rc::Rc;

/// Overview's column beside the description, and the least height it keeps when the
/// description is short.
const SIDE: f32 = 300.;
const SIDE_MIN: f32 = 320.;

/// What Overview is drawn with that the tab knows.
pub struct OverviewLook {
    /// The title, selectable as the tab makes it.
    pub title: AnyElement,
    /// The project's web address, which relative links in the description are relative to.
    pub base: Option<String>,
    /// The description's text selection.
    pub selecting: Option<Selecting>,
    pub facts: Facts,
    /// Opens the Discussions window: where an unresolved discussion in the way is fixed.
    pub discussions: Rc<dyn Fn(&mut App)>,
}

/// Overview's facts the tab keeps: the commits, how far your review is, the versions.
pub struct Facts {
    pub commits: usize,
    /// How far your own marks are: "3 of 8 files", or "—".
    pub reviewed: String,
    /// "v2 → v3", "v3", or "—".
    pub versions: String,
}

impl Requests {
    /// Overview: the identity and its facts, then the description beside the facts column.
    pub fn render_overview(&self, m: &Mr, look: OverviewLook, cx: &App) -> AnyElement {
        let d = self.details.clone().unwrap_or_default();
        // The list's own copy until the request's details are in.
        let text = if d.description.trim().is_empty() {
            &m.description
        } else {
            &d.description
        };
        let description = if text.trim().is_empty() {
            div()
                .text_color(theme::muted())
                .child("No description.")
                .into_any_element()
        } else {
            markdown::body(
                text,
                true,
                format!("overview-{}", m.iid),
                look.base.as_deref(),
                look.selecting,
                cx,
            )
        };
        island()
            .flex_1()
            .min_w_0()
            .bg(theme::editor())
            .child(
                div()
                    .id("overview")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.overview_identity(m, look.title, &look.facts))
                    .children(self.in_the_way(look.discussions.clone()))
                    .child(div().h(px(1.)).bg(theme::island_border()))
                    // The description sets the height; the facts and discussions beside it are no
                    // taller and scroll in their own column (the developer, 2026-10-23).
                    .child(
                        div()
                            .relative()
                            .px(px(24.))
                            .py(px(20.))
                            .child(
                                div()
                                    .mr(px(SIDE + 32.))
                                    .min_h(px(SIDE_MIN))
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(10.))
                                    .child(
                                        div()
                                            .text_size(px(15.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("Description"),
                                    )
                                    .child(div().text_size(px(13.)).child(description)),
                            )
                            .child(
                                div()
                                    .id("overview-side")
                                    .absolute()
                                    .top(px(20.))
                                    .bottom(px(20.))
                                    .right(px(24.))
                                    .w(px(SIDE))
                                    .overflow_y_scroll()
                                    .flex()
                                    .flex_col()
                                    .gap(px(20.))
                                    .text_size(px(12.))
                                    .children(self.overview_facts(&d, look.discussions)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Overview's head: the title and its state, its author and branches beside it, once (#190); then the facts in a
    /// row — pipeline, approvals, how far your own review is, the versions, the commits.
    fn overview_identity(&self, m: &Mr, title: AnyElement, facts: &Facts) -> Div {
        let url = m.url.clone();
        let mine = self.login.as_deref().is_some_and(|me| me == m.author_login);
        let pipeline = self.pipeline.as_ref().map_or_else(
            || faint("—").into_any_element(),
            |p| {
                let (glyph, word, color) = pipeline_word(p);
                div()
                    .text_color(color)
                    .child(format!("{glyph} {word}"))
                    .into_any_element()
            },
        );
        let approvals = match &self.approvals {
            Some(a) if a.required > 0 => {
                format!("{} of {}", a.required - a.left.min(a.required), a.required)
            }
            Some(a) if !a.by.is_empty() => format!("{} approved", a.by.len()),
            Some(_) => "none yet".to_owned(),
            None => "—".to_owned(),
        };
        let author = if mine {
            "You".to_owned()
        } else {
            m.author.clone()
        };
        div()
            .px(px(24.))
            .pt(px(20.))
            .pb(px(18.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        hover::cut_with("overview-title", m.title.clone(), title)
                            .min_w_0()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(state_chip(m))
                    // Who and from where to where, once, beside the title.
                    .child(
                        div()
                            .flex()
                            .min_w_0()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(13.))
                            .text_color(theme::muted())
                            .child(discussion::avatar(&m.author, false, 16.))
                            .child(div().flex_none().child(author))
                            .child(faint("·").flex_none())
                            .child(
                                hover::branches("overview-branches", &m.source, &m.target)
                                    .min_w_0()
                                    .font_family(theme::code_font())
                                    .text_size(px(12.)),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("overview-web")
                            .flex_none()
                            .text_size(px(12.))
                            .text_color(theme::faint())
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::accent()))
                            .child(words::say("Open in GitLab ↗").into_owned())
                            .on_click(move |_: &ClickEvent, _, cx| cx.open_url(&url)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(32.))
                    .gap_y(px(12.))
                    .child(fact("Pipeline", pipeline))
                    .child(fact(
                        "Approvals",
                        div()
                            .when(self.approval_waits(), |s| s.opacity(FADED))
                            .child(approvals)
                            .into_any_element(),
                    ))
                    // Your own marks, not the team's state (#190).
                    .child(fact(
                        "Your review",
                        div().child(facts.reviewed.clone()).into_any_element(),
                    ))
                    .child(fact(
                        "Versions",
                        div().child(facts.versions.clone()).into_any_element(),
                    ))
                    .child(fact(
                        "Commits",
                        div().child(facts.commits.to_string()).into_any_element(),
                    )),
            )
    }

    /// `In the way of merging`: a row for each reason, its facts and where it is fixed (#190, Design L4), between
    /// rules; none while nothing is in the way or the host's word is not in.
    fn in_the_way(&self, discussions: Rc<dyn Fn(&mut App)>) -> Option<Div> {
        let Some(Stand::InTheWay(blocks)) = self.merge_stand() else {
            return None;
        };
        let pipeline = self.pipeline.as_ref().map(|p| p.url.clone());
        let github = self.web.as_ref().is_some_and(|w| w.github);
        Some(
            div()
                .border_t_1()
                .border_color(theme::island_border())
                .px(px(24.))
                .py(px(16.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .text_size(px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::warning())
                        .child("IN THE WAY OF MERGING"),
                )
                .children(blocks.into_iter().enumerate().map(|(i, b)| {
                    let link = match &b.fix {
                        Fix::Nowhere => None,
                        Fix::Discussions => Some("Discussions →"),
                        Fix::Page(url) if Some(url) == pipeline.as_ref() => Some("Pipeline →"),
                        Fix::Page(_) if github => Some("GitHub ↗"),
                        Fix::Page(_) => Some("GitLab ↗"),
                    };
                    // A failure or a conflict is the red of what broke; a count to reach is plain.
                    let broke = matches!(b.what, "Pipeline" | "Conflicts");
                    let fix = b.fix.clone();
                    let discussions = discussions.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .text_size(px(13.))
                        .child(
                            div()
                                .flex_none()
                                .w(px(82.))
                                .text_color(theme::muted())
                                .child(b.what),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .text_color(if broke {
                                    theme::removed()
                                } else {
                                    theme::text()
                                })
                                .child(b.detail),
                        )
                        .children(link.map(|link| {
                            div()
                                .id(("overview-fix", i))
                                .flex_none()
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme::text()))
                                .child(link)
                                .on_click(move |_: &ClickEvent, _, cx| match &fix {
                                    Fix::Nowhere => {}
                                    Fix::Discussions => discussions(cx),
                                    Fix::Page(url) => cx.open_url(url),
                                })
                        }))
                })),
        )
    }

    /// Overview's column (#137, #190): who reviews and how far, the labels, the milestone, and the discussions —
    /// open and resolved, a click opening the Discussions window. A part with nothing to say is left out.
    fn overview_facts(&self, d: &Details, discussions: Rc<dyn Fn(&mut App)>) -> Vec<AnyElement> {
        let approved = self
            .approvals
            .as_ref()
            .map(|a| a.by.clone())
            .unwrap_or_default();
        // Who reviews: those asked, then whoever approved without being asked (GitHub drops an
        // approving reviewer from the asked ones).
        let mut reviewers = d.reviewers.clone();
        for name in approved.iter().chain(&d.changes_requested) {
            if !reviewers.contains(name) {
                reviewers.push(name.clone());
            }
        }
        let mut column = Vec::new();
        if !reviewers.is_empty() {
            column.push(
                block(
                    "Reviewers",
                    reviewers.into_iter().map(|r| {
                        let state = if approved.contains(&r) {
                            ("✓ approved", theme::added())
                        } else if d.changes_requested.contains(&r) {
                            ("requested changes", theme::warning())
                        } else {
                            ("reviewing", theme::muted())
                        };
                        let login = self.login_of(&r);
                        person(r, login, Some(state)).into_any_element()
                    }),
                )
                .into_any_element(),
            );
        }
        if !d.labels.is_empty() {
            column.push(
                block(
                    "Labels",
                    [div()
                        .flex()
                        .flex_wrap()
                        .gap(px(6.))
                        .children(d.labels.iter().map(|l| {
                            let color = l.color.map_or_else(theme::muted, gpui::rgb);
                            div()
                                .px(px(8.))
                                .h(px(20.))
                                .flex()
                                .items_center()
                                .rounded_full()
                                .border_1()
                                .border_color(Rgba { a: 0.5, ..color })
                                .bg(Rgba { a: 0.12, ..color })
                                .text_size(px(11.))
                                .text_color(theme::text())
                                .child(l.name.clone())
                        }))
                        .into_any_element()],
                )
                .into_any_element(),
            );
        }
        if let Some((title, due)) = &d.milestone {
            column.push(
                block(
                    "Milestone",
                    [div()
                        .flex()
                        .gap(px(6.))
                        .text_size(px(13.))
                        .child(title.clone())
                        .children(due.clone().map(|due| {
                            div().text_color(theme::faint()).child(format!("due {due}"))
                        }))
                        .into_any_element()],
                )
                .into_any_element(),
            );
        }
        let resolvable = self.threads.iter().filter(|t| t.resolvable);
        let open = resolvable.clone().filter(|t| !t.resolved).count();
        let resolved = resolvable.count() - open;
        if open + resolved > 0 {
            column.push(
                block(
                    "Discussions",
                    [div()
                        .id("overview-discussions")
                        .flex()
                        .items_center()
                        .text_size(px(13.))
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::accent()))
                        .child(format!("{open} open · {resolved} resolved"))
                        .child(div().flex_1())
                        .child(div().text_color(theme::accent()).child("→"))
                        .on_click(move |_: &ClickEvent, _, cx| discussions(cx))
                        .into_any_element()],
                )
                .into_any_element(),
            );
        }
        column
    }

    /// A request's identity in words, for a long hover: its number and title, whose it is and
    /// where it merges, its state, when it was opened and updated, who reviews it.
    pub fn request_tip(&self, iid: u64) -> Option<String> {
        let m = self.find(iid)?;
        let mut lines = vec![
            format!("{} {}", words::number(iid), m.title),
            format!("{} wants to merge {} → {}", m.author, m.source, m.target),
        ];
        let mut facts = Vec::new();
        if m.draft {
            facts.push("Draft".to_owned());
        }
        if m.comments > 0 {
            facts.push(plural(m.comments, "comment"));
        }
        facts.push(format!("updated {}", format::ago(m.updated)));
        lines.push(facts.join(" · "));
        if m.created > 0 {
            lines.push(format!("opened {}", hover::full_date(m.created)));
        }
        if !m.reviewers.is_empty() {
            // By name where it is known — the host's (#189), else a member's — you as you.
            let me = self.login.as_deref();
            let names: Vec<String> = m
                .reviewers
                .iter()
                .map(|login| {
                    let member = || self.members.iter().find(|(l, _)| l == login);
                    match m.reviewer_name(login) {
                        _ if Some(login.as_str()) == me => "you".to_owned(),
                        Some(name) => name.to_owned(),
                        None => match member() {
                            Some((_, name)) if !name.is_empty() => name.clone(),
                            _ => format!("@{login}"),
                        },
                    }
                })
                .collect();
            lines.push(format!("Reviewers: {}", names.join(", ")));
        }
        Some(lines.join("\n"))
    }
}

/// The request's state as a chip: open, a draft, merged or closed (#114).
fn state_chip(m: &Mr) -> Div {
    let (word, color) = match m.state {
        MrState::Merged => ("Merged", theme::accent()),
        MrState::Closed => ("Closed", theme::removed()),
        MrState::Opened if m.draft => ("Draft", theme::warning()),
        MrState::Opened => ("Open", theme::added()),
    };
    div()
        .flex_none()
        .px(px(6.))
        .rounded(px(4.))
        .bg(Rgba { a: 0.15, ..color })
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(word)
}

/// A fact of Overview's head: its label over its value.
fn fact(label: &'static str, value: AnyElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::faint())
                .child(label.to_uppercase()),
        )
        .child(div().text_size(px(13.)).child(value))
}

/// A block of Overview's column: its heading over its rows.
fn block(title: &str, rows: impl IntoIterator<Item = AnyElement>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::faint())
                .child(title.to_uppercase()),
        )
        .children(rows)
}

/// Someone in a list: the face, the name with the username under it (the developer,
/// 2026-10-23), and where they stand.
fn person(name: String, login: Option<String>, state: Option<(&'static str, Rgba)>) -> Div {
    let login = login.filter(|l| *l != name);
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(discussion::avatar(
            &name,
            false,
            if login.is_some() { 24. } else { 18. },
        ))
        .child(div().flex().flex_col().child(name).children(login.map(|l| {
            div()
                .text_size(px(11.))
                .text_color(theme::faint())
                .child(format!("@{l}"))
        })))
        .children(state.map(|(word, color)| div().text_color(color).child(word)))
}

fn faint(text: &'static str) -> Div {
    div().text_color(theme::faint()).child(text)
}
