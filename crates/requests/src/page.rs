//! A merge request's page, the requests' side of it (#73): Overview — the identity and its facts, the
//! description, a column of who reviews (#137) — and the one line of identity over Commits and Changes, with
//! the request's identity in words for a long hover. The tab puts them under the request's bar.

use crate::bar::pipeline_word;
use crate::requests::{Act, Acts, Requests};
use crate::waiting::FADED;
use artoo_hosts_api::{Details, Mr, MrState};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{discussion, island, px, theme, words};
use gpui::{AnyElement, App, ClickEvent, Div, FontWeight, Rgba, div, prelude::*};

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
}

/// Overview's facts the tab keeps: the commits, how far your review is, the versions.
pub struct Facts {
    pub commits: usize,
    /// "3 of 8 files", or "—".
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
                                    .children(self.overview_facts(&d)),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Overview's head: the title and its state, who wants to merge what into what, and the facts
    /// in a row — author, branches, pipeline, approvals, how far your review is, the versions.
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
        div()
            .px(px(24.))
            .pt(px(20.))
            .pb(px(18.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        hover::cut_with("overview-title", m.title.clone(), title)
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(state_chip(m))
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
                    .items_center()
                    .gap_x(px(5.))
                    .text_size(px(13.))
                    .text_color(theme::muted())
                    .child(div().text_color(theme::text()).child(if mine {
                        "You".to_owned()
                    } else {
                        m.author.clone()
                    }))
                    .child(match (m.state, mine) {
                        (MrState::Merged, _) => "merged",
                        (MrState::Closed, _) => "wanted to merge",
                        (MrState::Opened, true) => "want to merge",
                        (MrState::Opened, false) => "wants to merge",
                    })
                    .child(
                        div()
                            .text_color(theme::text())
                            .child(plural(facts.commits, "commit")),
                    )
                    .child("from")
                    .child(name("overview-source", &m.source).text_color(theme::accent()))
                    .child("into")
                    .child(name("overview-target", &m.target)),
            )
            .child(
                div()
                    .mt(px(12.))
                    .flex()
                    .flex_wrap()
                    .gap_x(px(32.))
                    .gap_y(px(12.))
                    .child(fact(
                        "Author",
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(discussion::avatar(&m.author, false, 16.))
                            .child(m.author.clone())
                            .into_any_element(),
                    ))
                    .child(fact(
                        "Branches",
                        div()
                            .flex()
                            .min_w_0()
                            .gap(px(5.))
                            .child(name("fact-source", &m.source).text_color(theme::accent()))
                            .child(faint("→").flex_none())
                            .child(name("fact-target", &m.target))
                            .into_any_element(),
                    ))
                    .child(fact("Pipeline", pipeline))
                    .child(fact(
                        "Approvals",
                        div()
                            .when(self.approval_waits(), |s| s.opacity(FADED))
                            .child(approvals)
                            .into_any_element(),
                    ))
                    .child(fact(
                        "Reviewed",
                        div().child(facts.reviewed.clone()).into_any_element(),
                    ))
                    .child(fact(
                        "Versions",
                        div().child(facts.versions.clone()).into_any_element(),
                    )),
            )
    }

    /// Overview's column: who reviews and how far (#137; the discussions are the Discussions
    /// window's).
    fn overview_facts(&self, d: &Details) -> Vec<AnyElement> {
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
        if reviewers.is_empty() {
            return Vec::new();
        }
        vec![
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
        ]
    }

    /// One line of a request's identity, over its Commits and its Changes: the title (`title`, as
    /// the tab makes it selectable), its state, the pipeline (#73).
    pub fn request_line(&self, m: &Mr, title: AnyElement, acts: Acts) -> Div {
        let iid = m.iid;
        island()
            // Its link and its number (#162).
            .on_mouse_down(gpui::MouseButton::Right, move |e, _, cx| {
                acts(
                    Act::RequestContext {
                        iid,
                        at: e.position,
                    },
                    cx,
                )
            })
            .flex_none()
            .flex_row()
            .items_center()
            .gap(px(10.))
            .h(px(40.))
            .px(px(14.))
            // Its whole identity in a long hover: the line names no author (the developer,
            // 2026-10-23).
            .child({
                match self.request_tip(m.iid) {
                    Some(tip) => div()
                        .id("request-line-title")
                        .min_w_0()
                        .cut_in_tip()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title)
                        .tip(tip),
                    None => hover::cut_with("request-line-title", m.title.clone(), title)
                        .font_weight(FontWeight::SEMIBOLD),
                }
            })
            .child(state_chip(m))
            .child(div().flex_1())
            .children(self.render_pipeline_pill())
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
            // By name where it is known, you as you.
            let me = self.login.as_deref();
            let names: Vec<String> = m
                .reviewers
                .iter()
                .map(
                    |login| match self.members.iter().find(|(l, _)| l == login) {
                        _ if Some(login.as_str()) == me => "you".to_owned(),
                        Some((_, name)) if !name.is_empty() => name.clone(),
                        _ => format!("@{login}"),
                    },
                )
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

/// A branch, cut in its middle to its room (#184).
fn name(id: &'static str, text: &str) -> gpui::Stateful<Div> {
    hover::name(id, text.to_owned()).font_family(theme::code_font())
}

fn faint(text: &'static str) -> Div {
    div().text_color(theme::faint()).child(text)
}
