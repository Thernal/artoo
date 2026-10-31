//! The head over a diff (#165, H3c): what it is of — a commit with its message, the working tree, a comparison with
//! why it may look wrong, an interdiff, two versions —, and the left of the diff's toolbar: which range is shown, as
//! the page offers it (the working tree's halves, a comparison's sides, the versions' tabs). A click asks the tab a
//! `history::Act`.

use crate::compare::only_pill;
use crate::history::{Act, Acts, History, VersionTab};
use artoo_core::git::WebRemote;
use artoo_review_api::diff::{Diff, Header};
use artoo_ui_kit::controls::{chip, group};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, Hover};
use artoo_ui_kit::{ROW_RADIUS, island, px, theme};
use gpui::{AnyElement, App, ClickEvent, FontWeight, IntoElement, Window, div, prelude::*};

/// What the head reads of the tab.
pub struct HeadLook<'a> {
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

/// The left of the diff's toolbar: which range is shown, as the page offers it. A comparison `narrowed` to a run of
/// its commits says so, its × the whole again (#21).
pub fn what(diff: &Diff, history: &History, narrowed: bool, acts: &Acts) -> AnyElement {
    match &diff.header {
        Header::Commit(c) => div()
            .text_size(px(12.))
            .text_color(theme::muted())
            .child(format!("{} · against its parent", format::short(c.id)))
            .into_any_element(),
        Header::WorkingTree { only_unstaged } => {
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
        Header::Compare {
            base,
            head,
            since_merge_base,
            request,
            ..
        } => {
            let (since, request) = (*since_merge_base, *request);
            let compare =
                |base: &(String, git2::Oid), head: &(String, git2::Oid), since, request| {
                    Act::Compare {
                        base: base.clone(),
                        head: head.clone(),
                        since,
                        request,
                    }
                };
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    group()
                        .child(
                            chip("cmp-since", "Since merge base", since)
                                .on_click(asks(acts, compare(base, head, true, request))),
                        )
                        .child(
                            chip("cmp-direct", "Direct", !since)
                                .on_click(asks(acts, compare(base, head, false, request))),
                        ),
                )
                .child(
                    chip("cmp-swap", "⇄ Swap", false)
                        .on_click(asks(acts, compare(head, base, since, None))),
                )
                .into_any_element()
        }
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
                    if let Some(b) = body.as_ref().filter(|_| !look.message_open) {
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
                .children(body.clone().filter(|_| look.message_open).map(|b| {
                    div()
                        .id("commit-body")
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .text_color(theme::muted())
                        .child(text(format!("commit-body-{}", commit.id), &b))
                }))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_x(px(12.))
                        .gap_y(px(2.))
                        .text_size(px(12.))
                        .text_color(theme::muted())
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(div().font_family(theme::code_font()).child(short))
                                .child(
                                    div()
                                        .id("copy-hash")
                                        .text_color(theme::accent())
                                        .cursor_pointer()
                                        .hover(|s| s.text_color(theme::text()))
                                        .child("copy")
                                        .on_click(asks(acts, Act::Copy(full))),
                                ),
                        )
                        .child(commit.author.clone())
                        .children(body.is_some().then(|| {
                            let acts = acts.clone();
                            div()
                                .id("body-toggle")
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .hover(|s| s.text_color(theme::text()))
                                .child(if look.message_open {
                                    "Message ▴"
                                } else {
                                    "Message ▾"
                                })
                                .on_click(move |e: &ClickEvent, _, cx| {
                                    if e.click_count() > 1 {
                                        return;
                                    }
                                    acts(Act::Message, cx)
                                })
                        }))
                        .child(format::date(commit.time))
                        .child(stats)
                        .children(look.web.map(|web| {
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
        }
        Header::WorkingTree { only_unstaged } => {
            let only = *only_unstaged;
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
                                .child(if only {
                                    "Working tree against the index"
                                } else {
                                    "Working tree against HEAD"
                                }),
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
                    chip("cmp-swap-hint", "⇄ Swap", false).on_click(asks(acts, swap)),
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
fn is_trailer(line: &str) -> bool {
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
    fn trailers_are_told_from_prose() {
        assert!(is_trailer("Co-Authored-By: Someone <a@b.c>"));
        assert!(is_trailer("signed-off-by: Someone"));
        assert!(!is_trailer("Fixes: the retry loop"));
        assert!(!is_trailer("A sentence: with a colon."));
        assert!(!is_trailer("no colon at all"));
    }
}
