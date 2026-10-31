//! The history's sections of the side panel (#49): the versions, newest first, and a commit's row. The tab draws
//! the panel around them; a click asks it an `Act`.

use crate::card::{CommitCard, deco_tags};
use crate::find::summary_text;
use crate::history::{Act, Acts, History, Selection};
use artoo_ui_kit::controls::tag;
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover};
use artoo_ui_kit::room::Room;
use artoo_ui_kit::{px, row, theme};
use gpui::{ClickEvent, Div, FontWeight, MouseButton, MouseDownEvent, div, prelude::*};

/// The versions, newest first, for the side panel's Versions section; those from `new_from` on arrived since the
/// user last looked.
pub fn version_rows(h: &History, new_from: Option<usize>, acts: &Acts) -> Div {
    let latest = h.versions.len() - 1;
    let (from, to) = match h.selection {
        Selection::Versions { from, to } => (Some(from), Some(to)),
        _ => (None, None),
    };
    // Who made a version is only worth a line when more than one person did.
    let several = h.versions.iter().any(|v| v.author != h.versions[0].author);
    div()
        .flex()
        .flex_col()
        .children(h.versions.iter().enumerate().rev().map(|(ix, v)| {
            let role = if Some(ix) == from {
                Some("from")
            } else if Some(ix) == to {
                Some("to")
            } else {
                None
            };
            let chosen = role.is_some();
            let kind = match format::reason(&v.reason) {
                "fetch" => "fetched",
                "unknown" => "version",
                k => k,
            };
            let mut detail: Vec<String> = Vec::new();
            if several {
                detail.push(v.author.clone());
            }
            detail.push(format::ago(v.time));
            match ix.checked_sub(1).and_then(|p| h.versions.get(p)) {
                None => detail.push(plural(v.commits, "commit")),
                Some(prev) => {
                    if prev.base != v.base {
                        detail.push(format!(
                            "onto {}",
                            v.base.map(format::short).unwrap_or_default()
                        ));
                    }
                    detail.push(match v.commits.cmp(&prev.commits) {
                        std::cmp::Ordering::Greater => {
                            format!("+{}", plural(v.commits - prev.commits, "commit"))
                        }
                        std::cmp::Ordering::Less => {
                            format!("−{}", plural(prev.commits - v.commits, "commit"))
                        }
                        std::cmp::Ordering::Equal => {
                            format!("same {}", plural(v.commits, "commit"))
                        }
                    });
                }
            }
            // The rail: a dot per version and a line to the older one.
            let rail = div()
                .flex_none()
                .relative()
                .w(px(14.))
                .h(px(44.))
                .when(ix > 0, |s| {
                    s.child(
                        div()
                            .absolute()
                            .left(px(6.5))
                            .top(px(18.))
                            .w(px(1.))
                            .h(px(30.))
                            .bg(theme::selected()),
                    )
                })
                .child(
                    div()
                        .absolute()
                        .left(px(3.))
                        .top(px(13.))
                        .size(px(8.))
                        .rounded_full()
                        .bg(if chosen {
                            theme::accent()
                        } else {
                            theme::faint()
                        }),
                );
            let acts = acts.clone();
            let item = row(("version", ix), chosen)
                // A long hover: who pushed it, exactly when, and what moved the branch (#53).
                .tip(format!(
                    "v{} · {} · {} · {}",
                    v.number,
                    v.author,
                    hover::full_date(v.time),
                    if v.reason.is_empty() {
                        kind
                    } else {
                        v.reason.as_str()
                    }
                ))
                .h(px(44.))
                .items_start()
                .gap_2()
                .child(rail)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .h_full()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme::accent())
                                        .child(format!("v{}", v.number)),
                                )
                                .child(kind.to_owned())
                                .child(
                                    div()
                                        .font_family(theme::code_font())
                                        .text_size(px(11.))
                                        .text_color(theme::muted())
                                        .child(format::short(v.tip)),
                                )
                                .child(div().flex_1())
                                .children(role.map(|r| tag(r, theme::accent())))
                                .when(ix == latest && role.is_none(), |s| {
                                    s.child(tag("latest", theme::faint()))
                                })
                                .when(new_from.is_some_and(|from| ix >= from), |s| {
                                    s.child(tag("new", theme::accent()))
                                }),
                        )
                        .child(
                            div()
                                .w_full()
                                .cut_in_tip()
                                .text_size(px(11.))
                                .text_color(theme::muted())
                                .child(detail.join(" · ")),
                        ),
                )
                .on_click(move |event: &ClickEvent, _, cx| {
                    let more = event.modifiers().secondary();
                    acts(Act::Version { ix, more }, cx)
                });
            div().px(px(6.)).py(px(1.)).child(item)
        }))
}

/// Commit `ix`'s row in the Commits section, the parts `room` leaves it; `card` on a long hover.
pub fn commit_row(
    h: &History,
    ix: usize,
    room: Room,
    card: Option<CommitCard>,
    acts: &Acts,
) -> Div {
    let commit = &h.commits[ix];
    let selected = h.selection == Selection::Commit(ix);
    // Its anatomy (#79): the title and sha always; who and when while the panel has 260 px;
    // its labels under 360 px leave first. Its card holds them all.
    let (menu, pick) = (acts.clone(), acts.clone());
    let item = row(("commit", ix), selected)
        .h(px(62.))
        .flex_col()
        .items_start()
        .justify_center()
        .child(
            div()
                .w_full()
                .line_height(px(17.))
                .clamp_in_tip(2)
                .child(summary_text(
                    commit.summary.clone().into(),
                    h.find.query.words(),
                )),
        )
        .child(
            div()
                .w_full()
                .flex()
                .gap_2()
                .text_size(px(11.))
                .text_color(theme::muted())
                .child(
                    div()
                        .font_family(theme::code_font())
                        .child(format::short(commit.id)),
                )
                .when(room.third(), |s| {
                    s.child(
                        div().flex().min_w_0().flex_shrink(1.).gap_2().children(
                            h.decor
                                .get(&commit.id)
                                // The first label and "+N" for the rest; the card names them all.
                                .map(|d| deco_tags(d))
                                .unwrap_or_default(),
                        ),
                    )
                })
                // A branch of one's own still has commits by others under it. The labels give
                // way first; the card has the whole name.
                .when(room.second(), |s| {
                    s.child(
                        div()
                            .flex_none()
                            .max_w(px(160.))
                            .cut_in_tip()
                            .child(commit.author.clone()),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .child(format::ago(commit.time)),
                    )
                }),
        )
        .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
            menu(
                Act::CommitMenu {
                    ix,
                    at: event.position,
                },
                cx,
            )
        })
        .on_click(move |_, _, cx| pick(Act::Commit(ix), cx));
    // A long hover: the commit's card (#53) — its whole message start, author, date, id, labels.
    let item = match card {
        Some(card) => {
            let card = std::rc::Rc::new(card);
            item.card(move |_, cx| CommitCard::show(&card, cx))
        }
        None => item,
    };
    div().w_full().h(px(64.)).px(px(6.)).py(px(1.)).child(item)
}
