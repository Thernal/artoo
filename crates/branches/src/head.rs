//! The head over a diff (#165, H3c): what it is of — a commit with its message, the working tree, a comparison with
//! why it may look wrong, an interdiff, two versions —, and the left of the diff's toolbar: which range is shown, as
//! the page offers it (the working tree's halves, a comparison's sides, the versions' tabs), and its words for zen's
//! line (`title`). A click asks the tab a `history::Act`. The head is a piece of the item page (`piece`), the last
//! identity there; the toolbar's left another (`bar_piece`).

use crate::compare::only_pill;
use crate::history::{Act, Acts, History, Selection, VersionTab};
use artoo_branches_api::{Range, RangeOn};
use artoo_core::git::WebRemote;
use artoo_review_api::diff::{Diff, Header};
use artoo_review_api::{Review, ToolbarFold};
use artoo_shell_api::{Piece, Services, Slot};
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::{ROW_RADIUS, icons, island, px, theme};
use artoo_ui_kit::{discussion, markdown};
use gpui::{AnyElement, App, ClickEvent, Entity, FontWeight, IntoElement, Window, div, prelude::*};
use std::rc::Rc;

/// The head over the diff shown (`Slot::Identity`), when no other part has said what the page is of: the diff the
/// review's (`Review` in the tab's `services`), a click on it the tab's answer to the history's acts (`Acts`). None
/// over a request's own Changes (#188).
pub fn piece(history: Entity<History>, services: Rc<Services>) -> Piece {
    Piece {
        id: "branches.head",
        slot: Slot::Identity,
        draw: Rc::new(move |route, drawing, cx| {
            // A new merge request being written is a page of its own: its bar says what it is.
            if (services.get::<Rc<dyn artoo_requests_api::Requests>>())
                .is_some_and(|r| r.creating(cx))
            {
                return None;
            }
            // A branch's commit has its message on its Overview; its Changes are the diff alone.
            if <artoo_branches_api::BranchCommitRoute as artoo_core::route::Place>::from_route(
                route,
            )
            .is_some()
            {
                return None;
            }
            let review = services.get::<Rc<dyn Review>>()?;
            let diff = review.shown(cx)?;
            let acts = services.get::<Acts>()?;
            let h = history.read(cx);
            // A request's own Changes, whole or since you read: the page bar says what it is, the toolbar is the
            // first row (#188). A commit of it keeps its head.
            if h.range_on(route, &*review, cx)
                .is_some_and(|r| r.on != Range::Commits)
            {
                return None;
            }
            // A commit of a request: the version it was pushed as, said beside its time.
            let of_request =
                <artoo_requests_api::RequestCommitRoute as artoo_core::route::Place>::from_route(
                    route,
                );
            let version = of_request.as_ref().and_then(|r| {
                let requests = services.get::<Rc<dyn artoo_requests_api::Requests>>()?;
                let (iid, versions) = requests.versions(cx)?;
                let tip = git2::Oid::from_str(&r.commit).ok()?;
                (iid == r.number)
                    .then(|| versions.iter().find(|v| v.tip == tip).map(|v| v.number))
                    .flatten()
            });
            let look = HeadLook {
                of_request: of_request.is_some(),
                version,
                message_open: h.message_open,
                web: drawing.web.as_ref(),
                compare_title: h.compare_title(route),
            };
            let cx: &App = cx;
            let text = |key: String, text: &str| {
                markdown::body(text, false, key, None, Some(drawing.selecting.clone()), cx)
            };
            let head = head(&diff, &look, &text, &acts);
            // The main line's page says where it is pushed and its latest commit over the commit's head (#208).
            let main =
                <artoo_branches_api::BranchRoute as artoo_core::route::Place>::from_route(route)
                    .filter(|b| h.main_line(&b.refname))
                    .and_then(|_| crate::facts::main_line(h));
            Some(match main {
                Some(line) => div()
                    .flex()
                    .flex_col()
                    .gap(px(artoo_ui_kit::gap()))
                    .child(line)
                    .child(head)
                    .into_any_element(),
                None => head,
            })
        }),
    }
}

/// The left of the diff's toolbar (`Slot::DiffBar`, `what`): the diff the review's, a comparison narrowed when a
/// run of its commits is picked on its page.
pub fn bar_piece(history: Entity<History>, services: Rc<Services>) -> Piece {
    Piece {
        id: "branches.what",
        slot: Slot::DiffBar,
        draw: Rc::new(move |route, _, cx| {
            let review = services.get::<Rc<dyn Review>>()?;
            let diff = review.shown(cx)?;
            let acts = services.get::<Acts>()?;
            let h = history.read(cx);
            let narrowed = h
                .compare_of(route)
                .is_some_and(|c| c.picked.is_some() && h.selection == Selection::None);
            // A request's own range takes the first place (#187); the count goes last, for want of room.
            if let Some(on) = h.range_on(route, &*review, cx) {
                let files = || files_count(diff.files.len(), on);
                let count = (review.toolbar_fold(cx) < ToolbarFold::Count).then(|| match on.on {
                    Range::Commits => match h.selection {
                        Selection::Commit(ix) if ix < h.commits.len() => Count::Words(format!(
                            "Commit {} of {}",
                            h.commits.len() - ix,
                            h.commits.len()
                        )),
                        _ => files(),
                    },
                    _ => files(),
                });
                return Some(range_switch(on, count, &acts));
            }
            // A branch's own page shows it since it left the main line: no sides to choose (#209).
            if <artoo_branches_api::BranchRoute as artoo_core::route::Place>::from_route(route)
                .is_some()
                && matches!(diff.header, Header::Compare { .. })
            {
                return None;
            }
            Some(what(&diff, h, narrowed, &acts))
        }),
    }
}

/// What the head reads of the tab.
pub struct HeadLook<'a> {
    /// The commit is one of a request's: its head reads `hash · version · time · author`.
    pub of_request: bool,
    /// The version of the request the commit was pushed as.
    pub version: Option<usize>,
    /// The commit's message open under its title.
    pub message_open: bool,
    /// Where the repository is on the web: a commit's page there.
    pub web: Option<&'a WebRemote>,
    /// A comparison narrowed to one of its commits: that commit's subject.
    pub compare_title: Option<String>,
}

/// Text selectable as the tab's comments are, by its key: a commit's title and message.
pub type Selectable<'a> = &'a dyn Fn(String, &str) -> AnyElement;

/// A click that asks the tab `act`.
fn asks(acts: &Acts, act: Act) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let acts = acts.clone();
    move |_, _, cx| acts(act.clone(), cx)
}

/// What stands after a request's range: how many files, a glyph and the number, or the commit of how many.
enum Count {
    Files(usize, String),
    Words(String),
}

/// `n` files of the range `on`, with the sentence that says them.
fn files_count(n: usize, on: RangeOn) -> Count {
    let tip = match (on.on, on.read) {
        (Range::Since, Some(v)) => {
            format!(
                "{} changed since v{v}, the version you read",
                plural(n, "file")
            )
        }
        _ => format!("{} changed", plural(n, "file")),
    };
    Count::Files(n, tip)
}

/// A request's range on its Changes (#187, Design A1): `All · Since v2 · Commits`, Since only when a push came after
/// the version you read, its tooltip saying so; then `count`, the files shown or the commit of how many.
fn range_switch(on: RangeOn, count: Option<Count>, acts: &Acts) -> AnyElement {
    let range = |id: &'static str, label: String, range: Range, tip: String| {
        chip(id, label, on.on == range)
            .tip(tip)
            .on_click(asks(acts, Act::Range(range)))
    };
    let key = if on.read.is_some() { "  s" } else { "" };
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            group()
                .child(range(
                    "range-all",
                    "All".into(),
                    Range::All,
                    format!("All changes{key}"),
                ))
                .children(on.read.map(|n| {
                    range(
                        "range-since",
                        format!("Since v{n}"),
                        Range::Since,
                        format!("Since v{n}, the version you read{key}"),
                    )
                }))
                .child(range(
                    "range-commits",
                    "Commits".into(),
                    Range::Commits,
                    "One commit at a time".into(),
                )),
        )
        .children(count.map(|count| {
            let words = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::muted());
            match count {
                Count::Files(n, tip) => words
                    .child(
                        icons::icon("files")
                            .size(px(13.))
                            .text_color(theme::muted()),
                    )
                    .child(n.to_string())
                    .id("range-count")
                    .tip(tip)
                    .into_any_element(),
                Count::Words(said) => words.child(said).into_any_element(),
            }
        }))
        .into_any_element()
}

/// The left of the diff's toolbar: which range is shown, as the page offers it. A comparison `narrowed` to a run of
/// its commits says so, its × the whole again (#21).
pub fn what(diff: &Diff, history: &History, narrowed: bool, acts: &Acts) -> AnyElement {
    match &diff.header {
        // The commit's head says what it is (#209).
        Header::Commit(_) => div().into_any_element(),
        // The branch checked out's whole work: one list, no halves (#209).
        Header::WorkingTree { since: Some(_), .. } => div().into_any_element(),
        Header::WorkingTree { only_unstaged, .. } => {
            let only = *only_unstaged;
            group()
                .child(
                    chip("wt-all", "All changes", !only)
                        .on_click(asks(acts, Act::OnlyUnstaged(false))),
                )
                .child(
                    chip("wt-unstaged", "Not staged only", only)
                        .on_click(asks(acts, Act::OnlyUnstaged(true))),
                )
                .into_any_element()
        }
        Header::Compare { .. } if narrowed => history
            .compare
            .as_ref()
            .and_then(|c| only_pill(c, acts))
            .map(IntoElement::into_any_element)
            .unwrap_or_else(|| div().into_any_element()),
        // A compare has no Since · Direct switch (Design, `branch.html?crange=b&cpick=1`): its sides are in View ▾ for
        // a request's, and a branch's own page shows it since it left the main line (#226).
        Header::Compare { .. } => div().into_any_element(),
        Header::Interdiff { from, to, .. } => chip("back-to-commits", "← Commits", false)
            .on_click(asks(
                acts,
                Act::Versions {
                    from: *from,
                    to: *to,
                },
            ))
            .into_any_element(),
        Header::Versions { .. } => group()
            .child(
                chip(
                    "tab-files",
                    "Files",
                    history.version_tab == VersionTab::Files,
                )
                .on_click(asks(acts, Act::VersionTab(VersionTab::Files))),
            )
            .child(
                chip(
                    "tab-commits",
                    format!("Commits · {}", diff.pairs.len()),
                    history.version_tab == VersionTab::Commits,
                )
                .on_click(asks(acts, Act::VersionTab(VersionTab::Commits))),
            )
            .into_any_element(),
    }
}

/// What the diff is of, in a few words: zen's line says it. A comparison narrowed to one of its commits is that
/// commit's subject (`compare_title`).
pub fn title(diff: &Diff, compare_title: Option<String>) -> String {
    match &diff.header {
        Header::Commit(c) => c.summary.clone(),
        Header::WorkingTree { .. } => "Working tree".to_owned(),
        Header::Compare { base, head, .. } => {
            compare_title.unwrap_or_else(|| format!("{} … {}", base.0, head.0))
        }
        Header::Interdiff { new, .. } => format!("Interdiff · {}", new.summary),
        Header::Versions { from, to, .. } => format!("v{} → v{}", from.number, to.number),
    }
}

/// The head row over the panes: the screen's identity.
pub fn head(diff: &Diff, look: &HeadLook, text: Selectable, acts: &Acts) -> AnyElement {
    island()
        .flex_none()
        .flex_row()
        .items_start()
        .gap_4()
        .px_4()
        .py(px(8.))
        .child(summary(diff, look, text, acts))
        .into_any_element()
}

/// A commit's message under its title, in its own scroll past 220 px.
fn commit_body(id: git2::Oid, body: &str, text: Selectable) -> gpui::Stateful<gpui::Div> {
    div()
        .id("commit-body")
        .max_h(px(220.))
        .overflow_y_scroll()
        .text_color(theme::muted())
        .child(text(format!("commit-body-{id}"), body))
}

fn summary(diff: &Diff, look: &HeadLook, text: Selectable, acts: &Acts) -> gpui::Div {
    let (added, removed) = diff
        .files
        .iter()
        .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
    let stats = div()
        .flex()
        .gap_2()
        .text_size(px(12.))
        .child(
            div()
                .text_color(theme::muted())
                .child(plural(diff.file_count(), "file")),
        )
        .child(div().text_color(theme::added()).child(format!("+{added}")))
        .child(
            div()
                .text_color(theme::removed())
                .child(format!("−{removed}")),
        );
    let header = div().flex_1().min_w_0().flex().flex_col().gap_1();
    match &diff.header {
        Header::Commit(commit) => {
            // The explanation under the title, without the trailers (Co-Authored-By …).
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
            let (full, short) = (commit.id.to_string(), format::short(commit.id));
            header
                // A long hover: the whole id, who and when in full, the message when it is
                // folded (the developer, 2026-10-23).
                .child({
                    let mut tip = vec![
                        commit.id.to_string(),
                        format!("{} <{}>", commit.author, commit.email),
                        hover::full_date(commit.time),
                    ];
                    if let Some(b) = body
                        .as_ref()
                        .filter(|_| !look.message_open && !look.of_request)
                    {
                        tip.push(String::new());
                        tip.extend(b.lines().take(12).map(str::to_owned));
                    }
                    div()
                        .id("commit-summary")
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(text(
                            format!("commit-summary-{}", commit.id),
                            &commit.summary,
                        ))
                        .tip(tip.join("\n"))
                })
                .children(
                    (body.clone())
                        .filter(|_| look.message_open && !look.of_request)
                        .map(|b| commit_body(commit.id, &b, &text)),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(12.))
                        .gap_y(px(2.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        // A click on the id copies the whole of it (#209).
                        .child(
                            div()
                                .id("copy-hash")
                                .font_family(theme::code_font())
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme::text()))
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .children(look.of_request.then(|| {
                                    icons::icon("commit")
                                        .size(px(13.))
                                        .text_color(theme::faint())
                                }))
                                .child(short)
                                .tip("Copy the whole id")
                                .on_click(asks(acts, Act::Copy(full))),
                        )
                        .children(look.of_request.then(|| {
                            div()
                                .flex()
                                .items_center()
                                .gap(px(12.))
                                .children(look.version.map(|n| {
                                    div()
                                        .id("commit-version")
                                        .flex()
                                        .items_center()
                                        .gap(px(4.))
                                        .child(
                                            icons::icon("layers")
                                                .size(px(13.))
                                                .text_color(theme::faint()),
                                        )
                                        .child(format!("v{n}"))
                                        .tip(format!("Pushed as version {n}"))
                                }))
                                .child(
                                    div()
                                        .id("commit-time")
                                        .child(format::short_ago(commit.time))
                                        .tip(hover::full_date(commit.time)),
                                )
                        }))
                        // A request's commit says who last, with their face (Design round 21, B3).
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .when(look.of_request, |s| {
                                    s.child(discussion::avatar(&commit.author, false, 16.))
                                        .text_color(theme::text())
                                })
                                .child(commit.author.clone()),
                        )
                        .children((body.is_some() && !look.of_request).then(|| {
                            let acts = acts.clone();
                            div()
                                .id("body-toggle")
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme::text()))
                                // How much more it says (#209).
                                .child({
                                    let n = body.as_deref().map_or(0, |b| b.lines().count());
                                    let more = format::plural(n, "more line");
                                    if look.message_open {
                                        format!("{more} ▴")
                                    } else {
                                        format!("{more} ▾")
                                    }
                                })
                                .on_click(move |e: &ClickEvent, _, cx| {
                                    if e.click_count() > 1 {
                                        return;
                                    }
                                    acts(Act::Message, cx)
                                })
                        }))
                        .children((!look.of_request).then(|| format::date(commit.time)))
                        .children((!look.of_request).then_some(stats))
                        .children(look.web.filter(|_| !look.of_request).map(|web| {
                            let url = web.commit(&commit.id.to_string());
                            div()
                                .id("open-web")
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme::text()))
                                .child(format!("Open in {} ↗", web.name()))
                                .on_click(asks(acts, Act::OpenUrl(url)))
                        })),
                )
                // Its message whole, under the facts.
                .children(
                    (body.filter(|_| look.of_request))
                        .map(|b| commit_body(commit.id, &b, &text).mt(px(6.))),
                )
        }
        Header::WorkingTree {
            only_unstaged,
            since,
        } => {
            let only = *only_unstaged;
            let against = match since {
                Some(since) => format!(
                    "Working tree against {}",
                    since.base.0.trim_start_matches("origin/")
                ),
                None if only => "Working tree against the index".to_owned(),
                None => "Working tree against HEAD".to_owned(),
            };
            header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_wrap()
                        .gap_x(px(12.))
                        .gap_y(px(4.))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(against),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(stats),
                )
        }
        Header::Compare {
            base,
            head,
            since_merge_base,
            start,
            commits,
            request,
            stale,
        } => {
            let since = *since_merge_base;
            // Why the result looks wrong, with the one click that fixes it.
            let warning = |text: String, button: gpui::Stateful<gpui::Div>| {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_3()
                    .py(px(6.))
                    .rounded(px(ROW_RADIUS))
                    .bg(theme::warning_bg())
                    .text_size(px(12.))
                    .text_color(theme::warning())
                    .child(div().flex_1().min_w_0().child(text))
                    .child(
                        button
                            .flex_none()
                            .bg(theme::hover())
                            .text_color(theme::text()),
                    )
            };
            let stale_note = stale.clone().map(|st| {
                let use_upstream = Act::Compare {
                    base: (st.upstream.clone(), st.tip),
                    head: head.clone(),
                    since,
                    request: *request,
                };
                warning(
                    format!(
                        "{} is {} behind {}: their changes show up here too",
                        base.0,
                        plural(st.behind, "commit"),
                        st.upstream
                    ),
                    chip("cmp-upstream", format!("Use {}", st.upstream), false)
                        .on_click(asks(acts, use_upstream)),
                )
            });
            let empty_note = (since && *commits == 0).then(|| {
                let swap = Act::Compare {
                    base: head.clone(),
                    head: base.clone(),
                    since,
                    request: None,
                };
                warning(
                    format!(
                        "{} has nothing {} lacks: are the sides swapped?",
                        head.0, base.0
                    ),
                    chip("cmp-swap-hint", "Swap", false).on_click(asks(acts, swap)),
                )
            });
            header
                .child(
                    div()
                        .flex()
                        .items_center()
                        .flex_wrap()
                        .gap_x(px(12.))
                        .gap_y(px(4.))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(
                                    look.compare_title
                                        .clone()
                                        .unwrap_or_else(|| format!("{} … {}", base.0, head.0)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(format!(
                            "{} · from {}",
                            plural(*commits, "commit"),
                            format::short(*start)
                        ))
                        .child(stats),
                )
                .children(stale_note)
                .children(empty_note)
        }
        Header::Interdiff { old, new, .. } => header
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_x(px(12.))
                    .gap_y(px(4.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Interdiff · {}", new.summary)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!(
                        "{} → {} · the old commit's patch against the new one's",
                        format::short(old.id),
                        format::short(new.id)
                    ))
                    .child(stats),
            ),
        Header::Versions {
            from,
            to,
            rebased,
            conflicts,
        } => header
            .child(
                div().flex().items_center().gap_3().child(
                    div()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("Changes from v{} to v{}", from.number, to.number)),
                ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!(
                        "v{} {} → v{} {}",
                        from.number,
                        format::short(from.tip),
                        to.number,
                        format::short(to.tip)
                    ))
                    .when(*rebased, |s| {
                        s.child(div().text_color(theme::warning()).child(format!(
                            "rebased onto {} — upstream changes hidden",
                            to.base.map(format::short).unwrap_or_default()
                        )))
                    })
                    .when(*conflicts > 0, |s| {
                        s.child(div().text_color(theme::removed()).child(format!(
                            "{} with rebase conflicts",
                            plural(*conflicts, "file")
                        )))
                    })
                    .child(stats),
            ),
    }
}

/// A `Key: value` line of the kind git calls a trailer.
pub(crate) fn is_trailer(line: &str) -> bool {
    const KEYS: [&str; 7] = [
        "co-authored-by",
        "signed-off-by",
        "reviewed-by",
        "acked-by",
        "tested-by",
        "reported-by",
        "cc",
    ];
    line.split_once(':')
        .is_some_and(|(key, _)| KEYS.contains(&key.trim().to_lowercase().as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_compare_draws_no_since_direct_switch() {
        let src = include_str!("head.rs");
        let body = src.split("#[cfg(test)]").next().unwrap();
        assert!(!body.contains(concat!("\"cmp-", "since\"")));
        assert!(!body.contains(concat!("\"cmp-", "direct\"")));
    }

    #[test]
    fn trailers_are_told_from_prose() {
        assert!(is_trailer("Co-Authored-By: Someone <a@b.c>"));
        assert!(is_trailer("signed-off-by: Someone"));
        assert!(!is_trailer("Fixes: the retry loop"));
        assert!(!is_trailer("A sentence: with a colon."));
        assert!(!is_trailer("no colon at all"));
    }
}
