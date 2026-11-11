//! The Branches list (#72): the working tree first, then the local branches with how far they are from the main
//! line and their request, a remote's branches folded under its name; the row picked previewed beside it. The tab
//! draws the list's island, its handle and the preview's around them; a click asks it an `Act`.

use crate::history::{Act, Acts, History};
use crate::roots::BranchRow;
use artoo_core::git::{ChangeKind, RefKind};
use artoo_ui_kit::controls::{empty_state, outline_button, primary_button, progress_badge};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover, RowTip};
use artoo_ui_kit::room::Room;
use artoo_ui_kit::{ROW_RADIUS, icons, px, theme, words};
use gpui::{
    AnyElement, AnyView, App, ClickEvent, Div, FontWeight, MouseButton, MouseDownEvent, Rgba,
    SharedString, Window, div, prelude::*,
};

/// A branch's card on a long hover.
pub type Card = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// What a branch's row and preview show that other features know.
#[derive(Default)]
pub struct Beside {
    /// How far your review of its tip is: files done of all (`artoo-review`).
    pub part: Option<(u32, u32)>,
    /// Your review of its tip is whole.
    pub reviewed: bool,
    /// Its request: its number and title.
    pub request: Option<(u64, String)>,
    pub card: Option<Card>,
}

/// What the list is drawn with that is not the history's.
pub struct ListLook<'a> {
    /// The room its rows have (#79).
    pub room: Room,
    /// The working tree's changes.
    pub changes: usize,
    /// The filter typed, trimmed: while there is one, the list is the matching branches alone.
    pub filter: &'a str,
    pub picked: Option<BranchRow>,
    /// What other features know of branch `ix`.
    pub beside: &'a dyn Fn(usize) -> Beside,
}

/// The list's rows, `rows` in the order it shows them: under a filter the matching branches, else the working tree,
/// the local branches and each remote under its name.
pub fn rows(h: &History, rows: &[BranchRow], look: &ListLook, acts: &Acts) -> Vec<AnyElement> {
    let mut list: Vec<AnyElement> = Vec::new();
    let row = |row: BranchRow, flat: bool| {
        branch_row(h, look, row, look.picked == Some(row), flat, acts).into_any_element()
    };
    if !look.filter.is_empty() {
        list.extend(rows.iter().map(|r| row(*r, true)));
        if rows.is_empty() {
            list.push(
                div()
                    .px(px(16.))
                    .py(px(10.))
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!("No branch matches “{}”.", look.filter))
                    .into_any_element(),
            );
        }
        return list;
    }
    list.push(heading("Working tree".to_owned()).into_any_element());
    list.push(row(BranchRow::WorkingTree, false));
    let local: Vec<usize> = h
        .branches
        .iter()
        .enumerate()
        .filter(|(_, b)| b.kind == RefKind::Local)
        .map(|(ix, _)| ix)
        .collect();
    list.push(heading(format!("Local · {}", local.len())).into_any_element());
    list.extend(
        local
            .into_iter()
            .map(|ix| row(BranchRow::Branch(ix), false)),
    );
    for (remote, ixs) in h.remote_branches() {
        let open = h.roots.open_remotes.contains(&remote);
        let (name, acts) = (remote.clone(), acts.clone());
        list.push(
            div()
                .id(SharedString::from(format!("remote-{remote}")))
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(12.))
                .pt(px(10.))
                .pb(px(4.))
                .text_size(px(11.))
                .text_color(theme::faint())
                .cursor_pointer()
                .hover(|s| s.text_color(theme::text()))
                .child(
                    icons::icon(if open { "chevron" } else { "chevron-right" })
                        .size(px(14.))
                        .text_color(theme::faint()),
                )
                .child(format!(
                    "{remote} · {}{}",
                    ixs.len(),
                    if open { "" } else { " · folded" }
                ))
                .on_click(move |e: &ClickEvent, _, cx| {
                    if e.click_count() < 2 {
                        acts(Act::ToggleRemote(name.clone()), cx)
                    }
                })
                .into_any_element(),
        );
        if open {
            list.extend(ixs.into_iter().map(|ix| row(BranchRow::Branch(ix), false)));
        }
    }
    list
}

/// Beside the list: the row picked previewed, or, with one local branch and nothing to commit, that state alone
/// (#185).
pub fn preview(h: &History, look: &ListLook, acts: &Acts) -> AnyElement {
    let mut local = h.branches.iter().filter(|b| b.kind == RefKind::Local);
    let alone = look.filter.is_empty() && local.clone().count() == 1 && look.changes == 0;
    if alone {
        let name = local.next().map(|b| b.name.clone()).unwrap_or_default();
        return empty_state(
            "branch",
            format!("Only {name}, nothing ahead"),
            String::new(),
            None,
        )
        .into_any_element();
    }
    match look.picked {
        Some(BranchRow::WorkingTree) => tree_preview(h, look.changes, acts).into_any_element(),
        Some(BranchRow::Branch(ix)) => branch_preview(h, ix, (look.beside)(ix).request, acts),
        None => div().into_any_element(),
    }
}

/// A row of Branches: the working tree's, or a branch's name, when, how far from the main
/// line, its request. `flat` (filtered) names a remote branch with its remote. Its anatomy
/// (#79): the name and ↑ ↓ always; its request and when while the list has 260 px; HEAD and
/// how far your review is under 360 px leave first; its card holds it all.
fn branch_row(
    h: &History,
    look: &ListLook,
    row: BranchRow,
    picked: bool,
    flat: bool,
    acts: &Acts,
) -> Div {
    let room = look.room;
    let head_name = h
        .branches
        .iter()
        .find(|b| b.is_head)
        .map(|b| b.name.clone());
    let mut card = None;
    let (id, icon, line1, line2): (SharedString, &'static str, AnyElement, Option<AnyElement>) =
        match row {
            BranchRow::WorkingTree => {
                let files = h.roots.tree.as_ref().map(|t| &t.1);
                let line1 = div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(if look.changes == 0 {
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Clean")
                            .into_any_element()
                    } else {
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(plural(look.changes, "change"))
                            .into_any_element()
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .cut_in_tip()
                            .text_color(theme::muted())
                            .child(if look.changes == 0 {
                                "nothing to commit"
                            } else {
                                "not committed"
                            }),
                    )
                    .children(head_name.filter(|_| room.second()).map(|n| {
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(format!("on {n}"))
                    }))
                    .into_any_element();
                let line2 = files.filter(|f| !f.is_empty()).map(|f| {
                    let names: Vec<&str> = f.iter().take(2).map(|f| f.path.as_str()).collect();
                    let more = f.len().saturating_sub(2);
                    div()
                        .cut_in_tip()
                        .child(if more > 0 {
                            format!("{} and {more} more", names.join(", "))
                        } else {
                            names.join(", ")
                        })
                        .into_any_element()
                });
                ("branch-row-wt".into(), "files", line1, line2)
            }
            BranchRow::Branch(ix) => {
                let b = &h.branches[ix];
                let beside = (look.beside)(ix);
                card = beside.card;
                let name = match (b.kind, flat) {
                    (RefKind::Remote, false) => {
                        b.name.split_once('/').map_or(b.name.as_str(), |(_, n)| n)
                    }
                    _ => b.name.as_str(),
                };
                let fact = h.roots.facts.get(&b.refname);
                let line1 = div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        hover::name(
                            SharedString::from(format!("branch-name-{ix}")),
                            name.to_owned(),
                        )
                        .flex_1()
                        .font_family(theme::code_font())
                        .when(b.kind == RefKind::Local, |s| {
                            s.font_weight(FontWeight::SEMIBOLD)
                        })
                        .when(b.kind == RefKind::Remote, |s| s.text_color(theme::muted())),
                    )
                    .children(
                        beside
                            .part
                            .filter(|_| room.third())
                            .map(|(done, total)| progress_badge(done, total)),
                    )
                    .children((room.third() && beside.reviewed).then(|| {
                        div()
                            .text_size(px(11.))
                            .text_color(theme::added())
                            .child("✓ reviewed")
                    }))
                    .children(fact.filter(|_| room.second()).map(|f| {
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .text_color(theme::faint())
                            .child(format::ago(f.time))
                    }))
                    .into_any_element();
                let mut parts: Vec<AnyElement> = Vec::new();
                if b.is_head && room.third() {
                    parts.push(
                        div()
                            .px(px(6.))
                            .rounded_full()
                            .border_1()
                            .border_color(theme::accent())
                            .text_size(px(10.))
                            .text_color(theme::accent())
                            .child("HEAD")
                            .into_any_element(),
                    );
                }
                match fact.and_then(|f| f.against.as_ref()) {
                    Some((up, 0, 0)) => parts.push(
                        div()
                            .child(format!("up to date with {up}"))
                            .into_any_element(),
                    ),
                    Some((_, ahead, behind)) => parts.push(
                        div()
                            .child(format!("↑{ahead} ↓{behind}"))
                            .into_any_element(),
                    ),
                    None => {}
                }
                if let Some((iid, _)) = beside.request.filter(|_| room.second()) {
                    let acts = acts.clone();
                    parts.push(
                        div()
                            .id(SharedString::from(format!("branch-mr-{ix}")))
                            .text_color(theme::accent())
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .child(format!("{} →", words::number(iid)))
                            .on_click(move |_: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                acts(Act::ShowRequest(iid), cx)
                            })
                            .into_any_element(),
                    );
                }
                let line2 = (!parts.is_empty()).then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .children(parts)
                        .into_any_element()
                });
                (
                    SharedString::from(format!("branch-row-{ix}")),
                    "branch",
                    line1,
                    line2,
                )
            }
        };
    // The working tree has no card: its tooltip names its files.
    let tree_tip = (row == BranchRow::WorkingTree).then(|| {
        let mut tip = vec![if look.changes == 0 {
            "Clean: nothing to commit".to_owned()
        } else {
            format!("{}, not committed", plural(look.changes, "change"))
        }];
        if let Some(files) = h.roots.tree.as_ref().map(|t| &t.1) {
            tip.extend(files.iter().take(12).map(|f| f.path.clone()));
            if files.len() > 12 {
                tip.push(format!("and {} more", files.len() - 12));
            }
        }
        tip.join("\n")
    });
    let (menu, pick) = (acts.clone(), acts.clone());
    div().px(px(6.)).py(px(1.)).child(
        div()
            .id(id)
            .px(px(10.))
            .py(px(7.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(picked, |s| s.bg(theme::selected()))
            .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
            .child(
                icons::icon(icon)
                    .mt(px(2.))
                    .size(px(15.))
                    .text_color(theme::faint()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(line1)
                    .children(
                        line2.map(|l| div().text_size(px(12.)).text_color(theme::muted()).child(l)),
                    ),
            )
            .when_some(card, |s, card| s.card(card))
            .when_some(tree_tip, |s, tip| s.row_tip(picked, tip))
            .when_some(
                match row {
                    BranchRow::Branch(ix) => Some(ix),
                    BranchRow::WorkingTree => None,
                },
                |s, ix| {
                    s.on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
                        menu(
                            Act::BranchMenu {
                                ix,
                                at: event.position,
                            },
                            cx,
                        )
                    })
                },
            )
            .on_click(move |event: &ClickEvent, _, cx| {
                let clicks = event.click_count();
                pick(Act::PickRow { row, clicks }, cx)
            }),
    )
}

/// The working tree picked, its `changes`: what is not committed, read only, and the way to its diff.
fn tree_preview(h: &History, changes: usize, acts: &Acts) -> Div {
    let head = h
        .branches
        .iter()
        .find(|b| b.is_head)
        .map(|b| b.name.clone());
    let files = h
        .roots
        .tree
        .as_ref()
        .map(|t| t.1.clone())
        .unwrap_or_default();
    let props = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            icons::icon("files")
                .size(px(15.))
                .text_color(theme::muted()),
        )
        .child(match &head {
            Some(n) => format!("Working tree · on {n}"),
            None => "Working tree".to_owned(),
        })
        .child(div().flex_1())
        .child(
            div()
                .text_color(theme::faint())
                .child("read only: commit in your editor or terminal"),
        );
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(if changes == 0 {
            "Nothing to commit".to_owned()
        } else {
            format!("{} not committed", plural(changes, "change"))
        });
    let list = (!files.is_empty()).then(|| {
        div()
            .flex()
            .flex_col()
            .py(px(4.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .children(files.into_iter().take(40).map(|f| {
                let (letter, color) = change_letter(f.change);
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(12.))
                    .h(px(26.))
                    .text_size(px(13.))
                    .child(
                        div()
                            .w(px(14.))
                            .text_size(px(11.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(letter),
                    )
                    .child(
                        hover::path(SharedString::from(format!("wt-file-{}", f.path)), f.path)
                            .flex_1(),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.))
                            .text_size(px(11.))
                            .child(
                                div()
                                    .text_color(theme::added())
                                    .child(format!("+{}", f.added)),
                            )
                            .child(
                                div()
                                    .text_color(theme::removed())
                                    .child(format!("−{}", f.removed)),
                            ),
                    )
            }))
    });
    let acts = acts.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(28.))
        .py(px(22.))
        .max_w(px(820.))
        .child(props)
        .child(title)
        .children(list)
        .children((changes > 0).then(|| {
            div()
                .flex()
                .child(primary_button("tree-open", "Open the diff", "↵").on_click(
                    move |_: &ClickEvent, _, cx| acts(Act::OpenRow(BranchRow::WorkingTree), cx),
                ))
        }))
}

/// Branch `ix` picked: its last commit, how far it is from the main line, its `request`, Open.
fn branch_preview(
    h: &History,
    ix: usize,
    request: Option<(u64, String)>,
    acts: &Acts,
) -> AnyElement {
    let Some(b) = h.branches.get(ix) else {
        return div().into_any_element();
    };
    let fact = h.roots.facts.get(&b.refname);
    let props = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            icons::icon("branch")
                .size(px(15.))
                .text_color(theme::muted()),
        )
        .child(match b.kind {
            RefKind::Local if b.is_head => "Local branch · checked out".to_owned(),
            RefKind::Local => "Local branch".to_owned(),
            RefKind::Remote => "Remote branch".to_owned(),
            RefKind::Request => "A merge request's head".to_owned(),
        })
        .child(div().flex_1())
        .children(fact.map(|f| {
            div()
                .text_color(theme::faint())
                .child(format!("last commit {}", format::ago(f.time)))
        }));
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .font_family(theme::code_font())
        .child(b.name.clone());
    let mut facts: Vec<String> = Vec::new();
    if let Some((against, ahead, behind)) = fact.and_then(|f| f.against.as_ref()) {
        facts.push(if (*ahead, *behind) == (0, 0) {
            format!("up to date with {against}")
        } else {
            format!(
                "{} ahead of {against}, {} behind",
                plural(*ahead, "commit"),
                behind
            )
        });
    }
    let last = fact.map(|f| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(12.))
            .py(px(8.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme::island_border())
            .bg(theme::panel())
            .child(div().text_size(px(13.)).child(f.summary.clone()))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme::muted())
                    .child(format!(
                        "{} · {} · {}",
                        format::short(b.tip),
                        f.author,
                        format::ago(f.time)
                    )),
            )
    });
    let open = acts.clone();
    let mut actions: Vec<AnyElement> = vec![
        primary_button("branch-open", "Open", "↵")
            .on_click(move |_: &ClickEvent, _, cx| open(Act::OpenRow(BranchRow::Branch(ix)), cx))
            .into_any_element(),
    ];
    if let Some((iid, _)) = &request {
        let (iid, acts) = (*iid, acts.clone());
        actions.push(
            div()
                .id("branch-request")
                .child(outline_button(&format!("{} →", words::number(iid))))
                .on_click(move |_: &ClickEvent, _, cx| acts(Act::ShowRequest(iid), cx))
                .into_any_element(),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(28.))
        .py(px(22.))
        .max_w(px(820.))
        .child(props)
        .child(title)
        .children(request.map(|(iid, title)| {
            div().text_size(px(13.)).text_color(theme::muted()).child(
                words::say(&format!(
                    "{} is open for this branch: {title}",
                    words::number(iid)
                ))
                .into_owned(),
            )
        }))
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(14.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .children(facts.into_iter().map(|f| div().child(f))),
        )
        .children(last)
        .child(div().flex().items_center().gap(px(10.)).children(actions))
        .into_any_element()
}

/// A group's heading in the list: "Local · 3".
fn heading(text: String) -> Div {
    div()
        .px(px(16.))
        .pt(px(10.))
        .pb(px(4.))
        .text_size(px(11.))
        .text_color(theme::faint())
        .child(text)
}

fn change_letter(change: ChangeKind) -> (&'static str, Rgba) {
    match change {
        ChangeKind::Added => ("A", theme::added()),
        ChangeKind::Deleted => ("D", theme::removed()),
        ChangeKind::Modified => ("M", theme::accent()),
        ChangeKind::Renamed => ("R", theme::renamed()),
        ChangeKind::Copied => ("C", theme::renamed()),
        ChangeKind::TypeChanged => ("T", theme::warning()),
    }
}
