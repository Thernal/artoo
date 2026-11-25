//! A merge request's page, the requests' side of it (#73): Overview — the title with its author and branches on one
//! line, the facts in a row, what is in the way of merging without a click (#190, Design L4), the description beside
//! headed cards: the description and the discussions, with who reviews and the labels (milestone among them) beside — and the request's identity in words for a
//! long hover. Commits and Changes have no line of their own over them: the title is in the page bar (#188).

use crate::bar::{pipeline_icon, pipeline_short, pipeline_word};
use crate::freshness::frames;
use crate::pagefresh::mark;
use crate::requests::{Act, Requests};
use crate::waiting::FADED;
use artoo_hosts_api::{Details, Draft, Mr, MrState, Thread};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::fresh;
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::markdown::{self, Selecting};
use artoo_ui_kit::{discussion, icons, island, px, theme, words};
use gpui::{AnyElement, App, ClickEvent, Div, FontWeight, Rgba, div, prelude::*};
use std::rc::Rc;
use std::time::Instant;

/// Details' column beside the cards, and the widest the cards' column grows (round 41, P1: headed cards as on
/// the Tasks issue page).
const SIDE: f32 = 264.;
const MAIN_MAX: f32 = 760.;
/// A card's header, as the Tasks page draws its sections.
const CARD_HEAD: f32 = 40.;

/// What a click on a card's row does.
type Opener = Rc<dyn Fn(&mut App)>;

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
    /// What a click on a discussion on a line of the diff asks the tab (#242).
    pub show: crate::requests::Acts,
}

/// Overview's facts the tab keeps: the commits, how far your review is, the versions.
pub struct Facts {
    pub commits: usize,
    /// How far your own marks are: files done of the total.
    pub reviewed: Option<(u32, u32)>,
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
        // Until the first read answers, a card's body is tints (round 41), after a moment of nothing.
        let now = Instant::now();
        let phase = self.page.fresh.phase(now);
        let loading = self.page.read_at.is_none() && (self.page.reading || phase.content < 1.);
        let moving = self.page.fresh.skeleton_moves(now);
        let side_loading = loading || (!self.page.side_in && self.page.reading);
        let tinted = |widths: &[f32]| skeleton_lines(widths, phase.skeleton);
        let description = match loading {
            true => tinted(&[520., 470., 560., 300.]).into_any_element(),
            false => description,
        };
        let discussions = match loading {
            true => Some(
                card("Discussions", None, tinted(&[140., 430., 160., 380.])).into_any_element(),
            ),
            false => self.overview_discussions(look.discussions.clone(), look.show.clone()),
        };
        let details = match side_loading {
            true => Some(
                card("Details", None, tinted(&[70., 130., 120., 60., 100.])).into_any_element(),
            ),
            false => self.overview_details(&d),
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
                    .track_scroll(&self.page.scroll)
                    .when(moving, |s| s.child(frames()))
                    .child(self.overview_identity(m, look.title, &look.facts))
                    .child(div().h(px(1.)).bg(theme::island_border()))
                    // The page scrolls as a whole: Description and Discussions as headed cards in the main
                    // column, Details beside them (round 41, P1).
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(24.))
                            .px(px(24.))
                            .pt(px(20.))
                            .pb(px(24.))
                            .child(
                                div().flex_1().min_w_0().child(
                                    div()
                                        .max_w(px(MAIN_MAX))
                                        .flex()
                                        .flex_col()
                                        .gap(px(16.))
                                        .child(card(
                                            "Description",
                                            None,
                                            div().text_size(px(13.)).child(description),
                                        ))
                                        .children(discussions),
                                ),
                            )
                            .children(details.map(|d| div().flex_none().w(px(SIDE)).child(d))),
                    ),
            )
            .into_any_element()
    }

    /// Overview's head (Design M2): the title and its state on a line of their own; the branches on the next; then
    /// the facts as glyphs, each with its sentence in its tooltip — pipeline, approvals, how far your own review is,
    /// the versions — the author last, the one that changes (#190).
    fn overview_identity(&self, m: &Mr, title: AnyElement, facts: &Facts) -> Div {
        let mine = self.login.as_deref().is_some_and(|me| me == m.author_login);
        let author = if mine { "You" } else { m.author.as_str() };
        let glyph = |id: &'static str, icon: &'static str, color, text: String, tip: String| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(color)
                .child(icons::icon(icon).size(px(14.)).text_color(color))
                .child(text)
                .tip(tip)
        };
        let divider = || {
            div()
                .flex_none()
                .w(px(1.))
                .h(px(14.))
                .bg(theme::island_border())
        };
        let mut row = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .text_size(px(13.))
            .text_color(theme::muted());
        if let Some(p) = &self.pipeline {
            let (icon, color) = pipeline_icon(p);
            let (_, said, _) = pipeline_word(p);
            let (_, short, _) = pipeline_short(p);
            row = row.child(glyph(
                "overview-pipeline",
                icon,
                color,
                short,
                format!("Pipeline {said}"),
            ));
        }
        let approvals = self.approvals.as_ref().and_then(|a| {
            let (count, said) = if a.required > 0 {
                let done = a.required - a.left.min(a.required);
                (
                    format!("{done}/{}", a.required),
                    format!("{done} of {} approvals", a.required),
                )
            } else if !a.by.is_empty() {
                (a.by.len().to_string(), format!("{} approved", a.by.len()))
            } else {
                return None;
            };
            Some(match a.by.is_empty() {
                true => (count, said),
                false => (count, format!("{said} · {}", a.by.join(", "))),
            })
        });
        if let Some((count, said)) = approvals {
            row = row.child(divider()).child(
                div()
                    .when(self.approval_waits(), |s| s.opacity(FADED))
                    .child(glyph(
                        "overview-approvals",
                        "circle-check",
                        theme::muted(),
                        count,
                        said,
                    )),
            );
        }
        // Your own marks, not the team's state (#190).
        if let Some((done, total)) = facts.reviewed {
            row = row.child(divider()).child(glyph(
                "overview-reviewed",
                "eye",
                theme::muted(),
                format!("{done}/{total}"),
                format!(
                    "Reviewed by you: {done} of {}",
                    plural(total as usize, "file")
                ),
            ));
        }
        if facts.versions != "\u{2014}" {
            let said = match facts.versions.split_once(" \u{2192} ") {
                Some((at, latest)) => format!("You reviewed {at}; the latest is {latest}"),
                None => format!("The latest version is {}", facts.versions),
            };
            row = row.child(divider()).child(glyph(
                "overview-versions",
                "layers",
                theme::muted(),
                facts.versions.clone(),
                format!("{said} · {}", plural(facts.commits, "commit")),
            ));
        }
        let row = row.child(divider()).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .min_w_0()
                .child(discussion::avatar(&m.author, mine, 16.))
                .child(hover::cut(("page-author", m.iid), author.to_owned()).min_w_0()),
        );
        div()
            .px(px(24.))
            .pt(px(20.))
            .pb(px(18.))
            .flex()
            .flex_col()
            .gap(px(10.))
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
                    .child(state_chip(m)),
            )
            // The branches, a line of their own: a long one is the subject.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(theme::accent())
                    .child(
                        icons::icon("branch")
                            .size(px(14.))
                            .text_color(theme::muted()),
                    )
                    .child(
                        hover::branches("overview-branches", &m.source, &m.target)
                            .min_w_0()
                            .font_family(theme::code_font()),
                    ),
            )
            .child(div().pt(px(4.)).child(row))
    }

    /// Details, the card at the right (round 41, P1): who reviews with where each stands as a glyph at the end of
    /// a 24 px row, and the labels with the milestone as a flag chip among them. A part with nothing to say is
    /// left out, and the card with it.
    fn overview_details(&self, d: &Details) -> Option<AnyElement> {
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
        let mut blocks = Vec::new();
        if !reviewers.is_empty() {
            blocks.push(block(
                "Reviewers",
                reviewers.into_iter().enumerate().map(|(ix, r)| {
                    let (icon, color, said) = if approved.contains(&r) {
                        ("circle-check", theme::added(), "approved")
                    } else if d.changes_requested.contains(&r) {
                        ("circle-x", theme::warning(), "requested changes")
                    } else {
                        ("eye", theme::muted(), "reviewing")
                    };
                    let login = self.login_of(&r);
                    reviewer_row(ix, r, login, icon, color, said).into_any_element()
                }),
            ));
        }
        if !d.labels.is_empty() || d.milestone.is_some() {
            let chip = |c: Rgba| {
                div()
                    .px(px(8.))
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded_full()
                    .border_1()
                    .border_color(Rgba { a: 0.5, ..c })
                    .bg(Rgba { a: 0.12, ..c })
                    .text_size(px(11.))
                    .text_color(theme::text())
            };
            blocks.push(block(
                "Labels",
                [div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.))
                    .children(d.labels.iter().map(|l| {
                        chip(l.color.map_or_else(theme::muted, gpui::rgb)).child(l.name.clone())
                    }))
                    // The milestone is a flag chip among them; its due date in the long hover.
                    .children(d.milestone.as_ref().map(|(title, due)| {
                        let tip = match due {
                            Some(due) => format!("Milestone {title} · due {due}"),
                            None => format!("Milestone {title}"),
                        };
                        chip(theme::muted())
                            .id("overview-milestone")
                            .child(icons::icon("flag").size(px(12.)).text_color(theme::muted()))
                            .child(title.clone())
                            .tip(tip)
                    }))
                    .into_any_element()],
            ));
        }
        if blocks.is_empty() {
            return None;
        }
        Some(
            card(
                "Details",
                None,
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.))
                    .text_size(px(12.))
                    .children(blocks),
            )
            .into_any_element(),
        )
    }

    /// The discussions still open as a headed card with their count, each thread in whole; one's own pending
    /// comments among them, marked; the resolved ones as a line that opens the window. Nothing to show leaves the
    /// card out.
    fn overview_discussions(
        &self,
        window: Rc<dyn Fn(&mut App)>,
        show: crate::requests::Acts,
    ) -> Option<AnyElement> {
        let open = open_threads(&self.threads);
        let resolved = self.threads.iter().filter(|t| t.resolved).count();
        let pending: Vec<&Draft> = self
            .drafts
            .iter()
            .filter(|d| d.reply_to.is_none())
            .collect();
        if open.is_empty() && pending.is_empty() && resolved == 0 {
            return None;
        }
        let me = self.login.as_deref();
        let mut rows = Vec::new();
        let (open_shown, new) = card_order(&self.threads, &self.page.new_notes);
        let mut new_rows = Vec::new();
        for (ix, t) in open_shown.iter().chain(&new).enumerate() {
            let Some(first) = t.notes.first() else {
                continue;
            };
            let place = t.path.as_ref().map(|p| {
                let line = t.new_line.or(t.old_line);
                let name = p.rsplit('/').next().unwrap_or(p);
                line.map_or_else(|| name.to_owned(), |n| format!("{name}:{n}"))
            });
            let waiting = self
                .drafts
                .iter()
                .any(|d| d.reply_to.as_deref() == Some(t.id.as_str()));
            let replies = t.notes.len() - 1;
            // A discussion on a line opens the diff at it; a general one opens the window (#242).
            let jump = t.path.is_some().then(|| {
                let (show, at) = (show.clone(), self.threads.iter().position(|x| x.id == t.id));
                Rc::new(move |cx: &mut App| {
                    if let Some(at) = at {
                        show(Act::ShowThread(at), cx)
                    }
                }) as Opener
            });
            let into = if ix < open_shown.len() {
                &mut rows
            } else {
                &mut new_rows
            };
            into.push(
                thread_row(
                    ("overview-thread", ix),
                    &first.author,
                    me == Some(first.login.as_str()),
                    (replies > 0).then(|| plural(replies, "reply")),
                    place,
                    waiting,
                    first.body.clone(),
                    window.clone(),
                    jump,
                )
                .relative()
                .child(mark(t.id.clone(), &self.page)),
            );
        }
        for (ix, d) in pending.iter().enumerate() {
            let (place, target, old) = self.draft_where(d);
            let jump = target.map(|(path, line)| {
                let show = show.clone();
                Rc::new(move |cx: &mut App| {
                    show(
                        Act::JumpToLine {
                            path: path.clone(),
                            line,
                            old,
                        },
                        cx,
                    )
                }) as Opener
            });
            rows.push(thread_row(
                ("overview-pending", ix),
                "You",
                true,
                None,
                (place != "General").then_some(place),
                true,
                d.body.clone(),
                window.clone(),
                jump,
            ));
        }
        // What came in under the page stands at the card's end, until it is gone to.
        rows.append(&mut new_rows);
        let mut body = div()
            .flex()
            .flex_col()
            .children(rows.into_iter().enumerate().map(|(i, r)| {
                // A hairline between the threads, none over the first.
                r.when(i > 0, |s| {
                    s.border_t_1().border_color(theme::island_border())
                })
            }));
        if resolved > 0 {
            let window = window.clone();
            body = body.child(
                div()
                    .id("overview-resolved")
                    .flex()
                    .items_center()
                    .py(px(10.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::accent()))
                    .child(format!("{resolved} resolved"))
                    .child(div().flex_1())
                    .child("→")
                    .on_click(move |_: &ClickEvent, _, cx| window(cx)),
            );
        }
        Some(card("Discussions", Some(open.len()), body).into_any_element())
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

/// The discussions of a request that are still open, in the host's order: what the Discussions card lists and counts.
pub(crate) fn open_threads(threads: &[Thread]) -> Vec<&Thread> {
    threads.iter().filter(|t| !t.resolved).collect()
}

/// The open threads in the card's order: those shown, then those that came in whole under the page and were not
/// gone to yet (their first note is new).
pub(crate) fn card_order<'a>(
    threads: &'a [Thread],
    new_notes: &[u64],
) -> (Vec<&'a Thread>, Vec<&'a Thread>) {
    (open_threads(threads).into_iter())
        .partition(|t| !t.notes.first().is_some_and(|n| new_notes.contains(&n.id)))
}

/// A card's body while it is read: still tints where the lines will be.
fn skeleton_lines(widths: &[f32], opacity: f32) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .opacity(opacity)
        .children(widths.iter().map(|w| {
            div()
                .h(px(18.))
                .flex()
                .items_center()
                .child(fresh::tint(px(*w).into(), 10.).max_w_full())
        }))
}

/// A headed card as the Tasks issue page draws its sections: a 40 px header with the title and its count, a
/// hairline, the body.
fn card(title: &str, count: Option<usize>, body: impl IntoElement) -> Div {
    div()
        .min_w_0()
        .rounded(px(6.))
        .border_1()
        .border_color(theme::island_border())
        .child(
            div()
                .h(px(CARD_HEAD))
                .flex()
                .items_center()
                .gap(px(8.))
                .pl(px(24.))
                .pr(px(8.))
                .rounded_t(px(6.))
                .border_b_1()
                .border_color(theme::island_border())
                .bg(theme::panel())
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_owned())
                .children(count.map(|n| {
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::NORMAL)
                        .text_color(theme::faint())
                        .child(n.to_string())
                })),
        )
        .child(
            div()
                .pt(px(12.))
                .pb(px(14.))
                .pl(px(24.))
                .pr(px(20.))
                .child(body),
        )
}

/// A block of Details: its heading over its rows.
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

/// One of Details' reviewers: the face and the name, where they stand as a glyph at the end; the username and
/// the word in the long hover.
fn reviewer_row(
    ix: usize,
    name: String,
    login: Option<String>,
    icon: &'static str,
    color: Rgba,
    said: &str,
) -> impl IntoElement {
    let who = match login.as_deref().filter(|l| *l != name) {
        Some(l) => format!("{name} (@{l})"),
        None => name.clone(),
    };
    div()
        .id(("overview-reviewer", ix))
        .h(px(24.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(discussion::avatar(&name, false, 18.))
        .child(div().min_w_0().child(name))
        .child(div().flex_1())
        .child(icons::icon(icon).size(px(14.)).text_color(color))
        .tip(format!("{who} · {said}"))
}

/// A discussion in its card: the face and the name, the place at the right (and the mark of a pending comment),
/// the whole text under them; a click opens the diff at it when it is on a line, else the Discussions window.
#[allow(clippy::too_many_arguments)]
fn thread_row(
    id: (&'static str, usize),
    author: &str,
    me: bool,
    replies: Option<String>,
    place: Option<String>,
    pending: bool,
    body: String,
    window: Rc<dyn Fn(&mut App)>,
    jump: Option<Opener>,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .gap(px(10.))
        .py(px(10.))
        .cursor_pointer()
        .child(
            div()
                .flex_none()
                .pt(px(1.))
                .child(discussion::avatar(author, me, 18.)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(px(12.))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(if me {
                            "You".to_owned()
                        } else {
                            author.to_owned()
                        }))
                        .children(replies.map(|r| div().text_color(theme::faint()).child(r)))
                        .child(div().flex_1())
                        .children(place.map(|p| {
                            div()
                                .flex_none()
                                .font_family(theme::code_font())
                                .text_size(px(11.))
                                .text_color(theme::faint())
                                .child(p)
                        }))
                        .when(pending, |s| {
                            s.child(
                                div()
                                    .id(("overview-pending-mark", id.1))
                                    .flex_none()
                                    .child(
                                        icons::icon("circle-dashed")
                                            .size(px(14.))
                                            .text_color(theme::muted()),
                                    )
                                    .tip("Pending: only you see it until the review is submitted"),
                            )
                        }),
                )
                .child(div().text_size(px(13.)).child(body)),
        )
        .on_click(move |_: &ClickEvent, _, cx| match &jump {
            Some(jump) => jump(cx),
            None => window(cx),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str, resolved: bool) -> Thread {
        Thread {
            id: id.to_owned(),
            notes: Vec::new(),
            path: None,
            new_line: None,
            old_line: None,
            resolved,
            resolvable: true,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        }
    }

    #[test]
    fn the_discussions_cards_count_is_the_open_threads() {
        let threads = [
            thread("a", false),
            thread("b", true),
            thread("c", false),
            thread("d", true),
        ];
        let open: Vec<&str> = open_threads(&threads)
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(open, ["a", "c"]);
        assert!(open_threads(&[]).is_empty());
    }
}
