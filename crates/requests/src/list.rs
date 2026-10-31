//! The requests list (#72, #81): grouped by what each needs from you, a group's band folding its rows; the folds and
//! the requests you have opened kept per project; the picked one's pipeline, approvals and what changed since your
//! review read for its preview. Its views draw the rows under their bands, the preview, why none is listed and the
//! banner while the list cannot be read; the tab puts them in the root's frame (`ListLook`, `Body`).

use crate::group::Group;
use crate::requests::{Act, Acts, Requests, Scope};
use artoo_core::git::{DiffSettings, Repo};
use artoo_core::{markdown, rank, storage};
use artoo_hosts_api::{Approvals, Host, Mr, MrState, Pipeline, Thread};
use artoo_review_api::Seen;
use artoo_ui_kit::controls::{
    empty_state, outline_button, pill, primary_button, progress_badge, unread_dot,
};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover};
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, discussion, flat, icons, motion, px, theme, words};
use gpui::{AnyElement, ClickEvent, Context, Div, FontWeight, SharedString, Task, div, prelude::*};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

/// What the requests list keeps per project (`<repo>\t<folded groups>\t<requests opened>`, both
/// comma-separated, then `folds` once a group was folded or unfolded by hand) (#81). Every group
/// starts open (the developer, 2026-10-23); a line written before the folds were yours carries the
/// old default (Reviewed folded), and is read as nothing folded.
const LIST_FILE: &str = "requests-list.txt";

/// The height of a group's band in the requests list.
pub const BAND: f32 = 28.;

/// Where request `seen` sits: yours first of all, then by how far your review got.
pub fn group(mine: bool, asks: bool, seen: Seen) -> Group {
    match seen {
        _ if mine => Group::Yours,
        Seen::Done => Group::Reviewed,
        Seen::Earlier => Group::Updated,
        Seen::Started => Group::Asks,
        Seen::Never if asks => Group::Asks,
        Seen::Never => Group::New,
    }
}

/// A request's pipeline and approvals, read for its preview, with its `updated` then.
pub struct Preview {
    updated: i64,
    pub pipeline: Option<Pipeline>,
    pub approvals: Option<Approvals>,
    /// Pushed since your review: the version you viewed, and how many files changed since then
    /// (X22).
    pub since: Option<(usize, usize)>,
}

/// The list's rows: their previews, its scroll, the groups folded and the requests opened.
#[derive(Default)]
pub struct Rows {
    previews: HashMap<u64, Preview>,
    preview_task: Option<Task<()>>,
    /// The requests list's scroll, and whether the next frame brings the picked row into view
    /// (after ↑ ↓ or esc back to the list).
    pub scroll: gpui::ScrollHandle,
    pub reveal: Cell<bool>,
    /// The request picked, previewed: by a click, by ↑ ↓, or the one open as you came back.
    pub picked: Option<u64>,
    /// Project `kept_of`'s requests list: the groups folded, and the requests you have opened —
    /// a row's dot is gone once you have (#81).
    folded: HashSet<Group>,
    opened: HashSet<u64>,
    /// A group was folded or unfolded by hand: the folds are kept.
    chosen: bool,
    kept_of: Option<String>,
}

impl Rows {
    /// Project `repo`'s folds and opened requests, unless they are the ones held.
    pub fn load(&mut self, repo: &str) {
        if self.kept_of.as_deref() == Some(repo) {
            return;
        }
        (self.folded, self.opened, self.chosen) =
            kept_list(&storage::read(LIST_FILE), repo).unwrap_or_default();
        self.kept_of = Some(repo.to_owned());
    }

    fn keep(&self) {
        if let Some(repo) = &self.kept_of {
            let text = storage::read(LIST_FILE);
            storage::save(
                LIST_FILE,
                with_kept_list(&text, repo, &self.folded, &self.opened, self.chosen),
            );
        }
    }

    /// Whether group `g`'s rows are folded away; a filter typed (`filtering`) opens every group.
    pub fn folds(&self, g: Group, filtering: bool) -> bool {
        !filtering && self.folded.contains(&g)
    }

    /// Group `g` folded or unfolded by hand.
    pub fn toggle(&mut self, g: Group) {
        self.chosen = true;
        if !self.folded.remove(&g) {
            self.folded.insert(g);
        }
        self.keep();
    }

    /// Whether request `iid` was opened here (#81).
    pub fn opened(&self, iid: u64) -> bool {
        self.opened.contains(&iid)
    }

    /// Request `iid`'s pipeline and approvals, when they were read.
    pub fn preview(&self, iid: u64) -> Option<&Preview> {
        self.previews.get(&iid)
    }
}

impl Requests {
    /// The requests the scope and `filter` let through, in the order the list shows them: the
    /// merged or the closed as read, newest first, the host having picked the scope's (#114,
    /// #169); the open by what they need from you (`seen`, how far your review of each got), the
    /// latest activity first. Copied: a row is drawn with the tab's context.
    pub fn shown(&self, filter: &str, seen: &dyn Fn(&Mr) -> Seen) -> Vec<Mr> {
        let filter = filter.to_lowercase();
        let terms: Vec<&str> = filter.split_whitespace().collect();
        let past = self.view != MrState::Opened;
        let list = if past {
            &self.past.of_scope(self.scope).list
        } else {
            &self.list
        };
        let mut shown: Vec<&Mr> = list
            .iter()
            .filter(|m| past || self.in_scope(m, self.scope))
            .filter(|m| {
                let hay = format!(
                    "{} {} {} {}",
                    words::number(m.iid),
                    m.title,
                    m.author,
                    m.source
                )
                .to_lowercase();
                terms.iter().all(|w| rank::word_matches(w, &hay))
            })
            .collect();
        if !past {
            shown.sort_by_key(|m| (self.group_of(m, seen(m)), std::cmp::Reverse(m.updated)));
        }
        shown.into_iter().cloned().collect()
    }

    /// Request `m`'s group in the list, `seen` how far your review of it got.
    pub fn group_of(&self, m: &Mr, seen: Seen) -> Group {
        let me = self.login.as_deref().unwrap_or_default();
        group(
            !me.is_empty() && m.author_login == me,
            !me.is_empty() && m.reviewers.iter().any(|r| r == me),
            seen,
        )
    }

    /// Of a request's `threads`, those waiting for you, its author (`waiting`).
    pub fn waiting_for_me<'a>(&self, threads: &'a [Thread]) -> Vec<&'a Thread> {
        waiting(self.login.as_deref().unwrap_or_default(), threads)
    }

    /// Request `iid` opened: its row's dot goes for good. Requests no longer open are forgotten.
    pub fn mark_opened(&mut self, iid: u64) {
        if self.rows.opened.contains(&iid) {
            return;
        }
        if !self.list.is_empty() {
            let list = &self.list;
            self.rows
                .opened
                .retain(|i| list.iter().any(|m| m.iid == *i));
        }
        self.rows.opened.insert(iid);
        self.rows.keep();
    }

    /// Request `m`, picked in the list: its pipeline and approvals, once per update of it; with
    /// `viewed` (its head now, and the one you reviewed it at), how many files changed since.
    pub fn read_preview(
        &mut self,
        m: &Mr,
        viewed: Option<(git2::Oid, git2::Oid)>,
        host: Arc<dyn Host>,
        root: PathBuf,
        settings: DiffSettings,
        cx: &mut Context<Self>,
    ) {
        let (iid, updated) = (m.iid, m.updated);
        if self
            .rows
            .previews
            .get(&iid)
            .is_some_and(|p| p.updated == updated)
        {
            return;
        }
        self.rows.preview_task = Some(cx.spawn(async move |this, cx| {
            let (pipeline, approvals, since) = cx
                .background_executor()
                .spawn(async move {
                    let since = viewed.and_then(|(head, viewed)| {
                        let pushes: Vec<_> = host
                            .versions(iid)
                            .ok()?
                            .iter()
                            .filter_map(|p| {
                                Some((
                                    git2::Oid::from_str(&p.head).ok()?,
                                    git2::Oid::from_str(&p.base).ok(),
                                    p.created,
                                ))
                            })
                            .collect();
                        let repo = Repo::open(&root).ok()?;
                        let versions = repo.versions_from_pushes(&pushes);
                        let from = versions.iter().find(|v| v.tip == viewed)?;
                        let to = versions.iter().find(|v| v.tip == head)?;
                        let diff = repo.version_diff(from, to, settings).ok()?;
                        Some((from.number, diff.files.len()))
                    });
                    (
                        host.pipeline(iid).ok().flatten(),
                        host.approvals(iid).ok(),
                        since,
                    )
                })
                .await;
            this.update(cx, |rq, cx| {
                rq.rows.previews.insert(
                    iid,
                    Preview {
                        updated,
                        pipeline,
                        approvals,
                        since,
                    },
                );
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Repository `repo`'s line of `requests-list.txt`: its folded groups (none unless folded by
/// hand), the requests opened, and whether the folds are yours; none when it has no line.
fn kept_list(text: &str, repo: &str) -> Option<(HashSet<Group>, HashSet<u64>, bool)> {
    let line = text.lines().find(|l| l.split('\t').next() == Some(repo))?;
    let parts: Vec<&str> = line.split('\t').skip(1).collect();
    let field = |n: usize| parts.get(n).copied().unwrap_or_default().split(',');
    let chosen = parts.get(2) == Some(&"folds");
    let folded = field(0)
        .filter_map(Group::parse)
        .filter(|_| chosen)
        .collect();
    let opened = field(1).filter_map(|iid| iid.parse().ok()).collect();
    Some((folded, opened, chosen))
}

/// `text` with repository `repo`'s line written anew; other repositories' kept.
fn with_kept_list(
    text: &str,
    repo: &str,
    folded: &HashSet<Group>,
    opened: &HashSet<u64>,
    chosen: bool,
) -> String {
    let mut folded: Vec<Group> = folded.iter().copied().collect();
    folded.sort();
    let mut opened: Vec<u64> = opened.iter().copied().collect();
    opened.sort();
    let line = format!(
        "{repo}\t{}\t{}{}",
        folded.iter().map(|g| g.key()).collect::<Vec<_>>().join(","),
        opened
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(","),
        if chosen { "\tfolds" } else { "" }
    );
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| !l.is_empty() && l.split('\t').next() != Some(repo))
        .map(str::to_owned)
        .chain([line])
        .collect();
    lines.sort_unstable();
    lines.join("\n") + "\n"
}

/// The band held at the top of the requests list while its rows scroll under it, by the bands'
/// tops and the list's top: the last band above it, pushed up as far as the next one comes into
/// its room (#81).
pub fn held_band(tops: &[f32], top: f32) -> Option<(usize, f32)> {
    let at = tops.iter().rposition(|&t| t < top)?;
    let push = tops
        .get(at + 1)
        .map_or(0., |&next| (next - top - BAND).min(0.));
    Some((at, push))
}

/// Of a request's `threads`, those waiting for `me`, its author: the last word not mine, and —
/// a discussion — still open, or — a plain comment, which cannot be resolved (X21, the developer,
/// 2026-10-23) — written after anything I wrote on the request.
fn waiting<'a>(me: &str, threads: &'a [Thread]) -> Vec<&'a Thread> {
    if me.is_empty() {
        return Vec::new();
    }
    let my_last = threads
        .iter()
        .flat_map(|t| &t.notes)
        .filter(|n| n.login == me)
        .map(|n| n.created)
        .max()
        .unwrap_or(0);
    threads
        .iter()
        .filter(|t| match t.notes.last() {
            Some(last) if last.login != me => {
                if t.resolvable {
                    !t.resolved
                } else {
                    last.created > my_last
                }
            }
            _ => false,
        })
        .collect()
}

// ---- the views ------------------------------------------------------------------------------

/// What the list draws that the tab knows.
pub struct ListLook<'a> {
    /// The requests listed, in the list's order: the open by what they need, the merged or the
    /// closed newest first.
    pub shown: &'a [Mr],
    /// The row picked (a click, ↑ ↓), and the filter typed in the page bar.
    pub picked: Option<u64>,
    pub filter: &'a str,
    /// Narrow: no preview beside the list, and a click opens a request (#80).
    pub compact: bool,
    /// Comments drawn as markdown: a waiting discussion's line says them so.
    pub rich: bool,
    /// The lists' widths: a row shows what its list has room for (#79).
    pub widths: &'a Widths,
    /// How far your review of each request listed got, and how many of its files are reviewed
    /// at its head (done, total).
    pub seen: HashMap<u64, Seen>,
    pub parts: HashMap<u64, (u32, u32)>,
    /// A request's discussions, as the notifications read them.
    pub threads: &'a dyn Fn(u64) -> Option<&'a [Thread]>,
    /// A row folding with its group (#110): by its key, its group's motion and whether it is
    /// open; `None` once it is shut.
    pub fold: &'a dyn Fn(String, &str, bool, AnyElement) -> Option<AnyElement>,
}

impl ListLook<'_> {
    fn seen(&self, iid: u64) -> Seen {
        self.seen.get(&iid).copied().unwrap_or(Seen::Never)
    }

    /// Whether the band's chevrons fold: not while a filter is typed.
    fn filtering(&self) -> bool {
        !self.filter.trim().is_empty()
    }
}

/// The list as it is drawn.
pub enum Body {
    /// Nothing read yet.
    Loading,
    /// No request to list, and why.
    Empty(Div),
    /// The rows under their bands, one held at the top as they scroll; the picked one's preview
    /// beside them unless the tab is narrow; whether the list's end, a past page's shimmer, is in
    /// sight (`Some(true)`: the next page is read) or not laid out yet (`Some(false)`: looked at
    /// again once it is).
    List {
        rows: Div,
        preview: Option<AnyElement>,
        more: Option<bool>,
    },
}

/// Whether you have been inside request `iid`: opened here, or files of it marked (#81).
pub fn opened_before(rq: &Requests, iid: u64, seen: Seen) -> bool {
    rq.rows.opened(iid) || !matches!(seen, Seen::Never)
}

/// Merge requests' root: the list grouped, the picked one previewed; or why there is none.
pub fn list(rq: &Requests, look: &ListLook, acts: &Acts) -> Body {
    let shown = look.shown;
    // The merged or the closed (#114): a flat list, newest first, read as it is scrolled.
    let past = (rq.view != MrState::Opened).then(|| rq.past.of_scope(rq.scope));
    let loading = match past {
        Some(p) => p.loading && p.list.is_empty(),
        None => rq.loading && rq.list.is_empty(),
    };
    if loading {
        return Body::Loading;
    }
    if shown.is_empty() && past.is_none_or(|p| !p.more) {
        return Body::Empty(empty(rq, look, acts));
    }
    let groups: Vec<Group> = match past {
        Some(_) => vec![Group::New; shown.len()],
        None => shown
            .iter()
            .map(|m| rq.group_of(m, look.seen(m.iid)))
            .collect(),
    };
    let folds = |g: Group| rq.rows.folds(g, look.filtering());
    let picked = look
        .picked
        .and_then(|iid| shown.iter().find(|m| m.iid == iid))
        .or_else(|| {
            shown
                .iter()
                .zip(&groups)
                .find(|(_, g)| past.is_some() || !folds(**g))
                .map(|(m, _)| m)
        });
    // Each group a band, its rows under it unless it is folded (#81).
    let mut rows: Vec<AnyElement> = Vec::new();
    let mut bands: Vec<(usize, Group, usize)> = Vec::new();
    let mut picked_row = None;
    for (n, (m, &g)) in shown.iter().zip(&groups).enumerate() {
        if past.is_some() {
            if picked.is_some_and(|p| p.iid == m.iid) {
                picked_row = Some(rows.len());
            }
            rows.push(row(m, g, rq, look, acts).into_any_element());
            continue;
        }
        if n == 0 || groups[n - 1] != g {
            let count = groups.iter().filter(|&&o| o == g).count();
            bands.push((rows.len(), g, count));
            let top = rows.is_empty();
            rows.push(band(g, count, false, top, rq, look, acts).into_any_element());
        }
        let is_picked = picked.is_some_and(|p| p.iid == m.iid);
        // Each row folds with its group, in size and alpha (#110): the group opens and shuts as
        // one, and every row stays a child of its own for the held band.
        let Some(row) = (look.fold)(
            format!("request-row-{}", m.iid),
            &format!("group-{}", g.key()),
            !folds(g) || is_picked,
            row(m, g, rq, look, acts).into_any_element(),
        ) else {
            continue;
        };
        if is_picked {
            picked_row = Some(rows.len());
        }
        rows.push(row);
    }
    // More of them while there may be: a shimmer row at the end, read when it comes into sight
    // (#114).
    if let Some(error) = past.and_then(|p| p.error.as_ref()) {
        let acts = acts.clone();
        rows.push(
            div()
                .px(px(16.))
                .py(px(12.))
                .flex()
                .items_center()
                .gap(px(10.))
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(hover::cut("past-error", error.clone()).flex_1())
                .child(
                    div()
                        .id("past-retry")
                        .child(outline_button("Retry"))
                        .on_click(move |_: &ClickEvent, _, cx| acts(Act::RetryPast, cx)),
                )
                .into_any_element(),
        );
    } else if past.is_some_and(|p| p.more || p.loading) {
        rows.push(
            div()
                .id("past-more")
                .px(px(16.))
                .py(px(12.))
                .child(motion::shimmer("past-more-bars", 2))
                .into_any_element(),
        );
    }
    // The rows as the last frame laid them out, when they are these rows: where the list's top
    // is, so the band above it is held there.
    let scroll = &rq.rows.scroll;
    let laid = scroll.children_count() == rows.len();
    let mut more = None;
    if past.is_some_and(|p| p.more && !p.loading) {
        // Not laid out yet: looked at again once it is. A row's bounds are where it is in the
        // unscrolled list.
        let near = laid
            && scroll.bounds_for_item(rows.len() - 1).is_some_and(|b| {
                b.top() + scroll.offset().y < scroll.bounds().bottom() + px(BAND * 4.)
            });
        if near || !laid {
            more = Some(near);
        }
    }
    let top = f32::from(scroll.bounds().top() - scroll.offset().y);
    let top_of = |ix: usize| scroll.bounds_for_item(ix).map(|b| f32::from(b.top()));
    if rq.rows.reveal.take()
        && let Some(ix) = picked_row
    {
        // A row going up out of sight would stop under the held band: the one above it comes
        // into view instead, and with it the row.
        let under_band = laid && ix > 0 && top_of(ix).is_some_and(|t| t < top + BAND);
        scroll.scroll_to_item(if under_band { ix - 1 } else { ix });
    }
    let tops: Vec<f32> = bands.iter().filter_map(|&(ix, ..)| top_of(ix)).collect();
    let held = (laid && tops.len() == bands.len())
        .then(|| held_band(&tops, top))
        .flatten()
        .map(|(at, push)| {
            let (_, g, count) = bands[at];
            div()
                .absolute()
                .top(px(push))
                .left_0()
                .right_0()
                .child(band(g, count, true, true, rq, look, acts))
        });
    let rows = div()
        .relative()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(look.widths.measure("requests"))
        .child(
            div()
                .id("requests")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(scroll)
                .pb(px(6.))
                .children(rows),
        )
        .children(held);
    Body::List {
        rows,
        preview: picked
            .filter(|_| !look.compact)
            .map(|m| preview(m, rq, look, acts)),
        more,
    }
}

/// No request to list: why, per filter, and the way to more.
fn empty(rq: &Requests, look: &ListLook, acts: &Acts) -> Div {
    let all = rq.list.len();
    let filter = look.filter.trim();
    let view = rq.view;
    let show_all = |label: &'static str| {
        let acts = acts.clone();
        div()
            .id("empty-show-all")
            .child(outline_button(label))
            .on_click(move |_: &ClickEvent, _, cx| acts(Act::ShowAll, cx))
            .into_any_element()
    };
    if !filter.is_empty() {
        return empty_state(
            "search",
            format!("Nothing matches “{filter}”"),
            words::say(
                "The filter looks in the number, title, author and branch of each merge request.",
            )
            .into_owned(),
            Some(show_all("Clear the filter")),
        );
    }
    // The merged or the closed (#114): none read, or none of this filter's.
    if view != MrState::Opened {
        let word = view.word().to_lowercase();
        let past = rq.past.of_scope(rq.scope);
        if let Some(error) = &past.error {
            let acts = acts.clone();
            return empty_state(
                "pull-request",
                words::say(&format!("The {word} merge requests could not be read")).into_owned(),
                error.clone(),
                Some(
                    div()
                        .id("past-retry")
                        .child(outline_button("Retry"))
                        .on_click(move |_: &ClickEvent, _, cx| acts(Act::RetryPast, cx))
                        .into_any_element(),
                ),
            );
        }
        let read = !rq.past.of_scope(Scope::All).list.is_empty();
        return match rq.scope {
            Scope::All => empty_state(
                "pull-request",
                words::say(&format!("No {word} merge requests")).into_owned(),
                words::say(&format!("Merge requests {word} in this project land here."))
                    .into_owned(),
                None,
            ),
            Scope::ReviewMe => empty_state(
                "pull-request",
                format!("None {word} asked for your review"),
                words::say(&format!(
                    "{} merge requests you were asked to review land here.",
                    view.word()
                ))
                .into_owned(),
                read.then(|| show_all("Show all")),
            ),
            Scope::Mine => empty_state(
                "pull-request",
                format!("None of yours is {word}"),
                words::say(&format!(
                    "The merge requests you opened land here once {word}."
                ))
                .into_owned(),
                read.then(|| show_all("Show all")),
            ),
        };
    }
    if rq.error.is_some() && all == 0 {
        return empty_state(
            "pull-request",
            words::say("No merge requests yet").into_owned(),
            "They come when the list can be read again.".to_owned(),
            None,
        );
    }
    match rq.scope {
        Scope::ReviewMe => empty_state(
            "pull-request",
            "Nothing to review".to_owned(),
            format!(
                "Requests that ask for your review land here. {} open under All.",
                if all == 1 {
                    "1 is".to_owned()
                } else {
                    format!("{all} are")
                }
            ),
            (all > 0).then(|| show_all("Show all")),
        ),
        Scope::Mine => empty_state(
            "pull-request",
            "None of yours is open".to_owned(),
            words::say("The merge requests you open land here, with what waits for you on each.")
                .into_owned(),
            (all > 0).then(|| show_all("Show all")),
        ),
        Scope::All => empty_state(
            "pull-request",
            words::say("No open merge requests").into_owned(),
            words::say("When someone opens a merge request in this project, it lands here.")
                .into_owned(),
            None,
        ),
    }
}

/// A group's band in the requests list: its icon, name and count, and the chevron that folds it
/// (a click anywhere on it does); `held` is its copy held at the top of the list (#81).
fn band(
    g: Group,
    count: usize,
    held: bool,
    top: bool,
    rq: &Requests,
    look: &ListLook,
    acts: &Acts,
) -> impl IntoElement + use<> {
    let (icon, tone) = g.icon();
    // At the list's top its fill follows the island's round corners, and the island's own border
    // is its top edge.
    let corner = if flat() { 0. } else { ISLAND_RADIUS - 1. };
    let foldable = !look.filtering();
    let folded = rq.rows.folds(g, look.filtering());
    let acts = acts.clone();
    div()
        .id((if held { "group-held" } else { "group-band" }, g as usize))
        .h(px(BAND))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .pl(px(16.))
        .pr(px(10.))
        .bg(theme::base())
        .when(top, |s| s.rounded_t(px(corner)))
        .when(!top, |s| s.border_t_1())
        .border_b_1()
        .border_color(theme::island_border())
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(if folded {
            theme::muted()
        } else {
            theme::text()
        })
        .child(icons::icon(icon).size(px(14.)).text_color(tone))
        .child(hover::cut("heading", g.heading()).flex_1())
        .child(
            div()
                .flex_none()
                .px(px(6.))
                .rounded_full()
                .bg(theme::hover())
                .text_size(px(11.))
                .font_weight(FontWeight::NORMAL)
                .text_color(theme::muted())
                .child(count.to_string()),
        )
        .when(foldable, |s| {
            s.cursor_pointer()
                .child(
                    icons::icon(if folded { "chevron-right" } else { "chevron" })
                        .size(px(14.))
                        .text_color(theme::faint()),
                )
                .on_click(
                    // A double click folds or opens it once, as one click does.
                    move |e: &ClickEvent, _, cx| {
                        if e.click_count() < 2 {
                            acts(Act::ToggleGroup(g), cx)
                        }
                    },
                )
        })
}

/// A request's row: unread dot, number and title, when; who, the pipeline, its discussions.
fn row(m: &Mr, g: Group, rq: &Requests, look: &ListLook, acts: &Acts) -> impl IntoElement + use<> {
    let iid = m.iid;
    let picked = look.picked == Some(iid);
    let part = look.parts.get(&iid).copied();
    let pipeline = rq.rows.preview(iid).and_then(|p| p.pipeline.as_ref());
    let threads = (look.threads)(iid);
    let waiting = (g == Group::Yours).then(|| rq.waiting_for_me(threads.unwrap_or_default()).len());
    let threads = threads.map(<[Thread]>::len);
    // A dot: a row of yours a thread waits on, another you have never opened; how far your review
    // is, its group says (#81).
    let unread = m.state == MrState::Opened
        && match waiting {
            Some(n) => n > 0,
            None => !opened_before(rq, iid, look.seen(iid)),
        };
    // Its anatomy (#79): the number, the title and the pipeline's verdict always; who (or what
    // waits for you) and when while the list has 260 px; the threads and how far your review is
    // under 360 px leave first. The tooltip holds it all.
    let room = look.widths.room("requests");
    let who = match waiting {
        Some(n) if n > 0 => Some((
            format!(
                "{} for you",
                if n == 1 {
                    "1 thread waits".to_owned()
                } else {
                    format!("{n} threads wait")
                }
            ),
            theme::warning(),
        )),
        _ => Some((
            if m.draft {
                format!("draft · {}", m.author)
            } else {
                m.author.clone()
            },
            theme::muted(),
        )),
    };
    let verdict = pipeline.map(crate::bar::pipeline_word);
    let threads = threads.filter(|&n| n > 0 && waiting.is_none_or(|w| w == 0));
    let mut tip = vec![
        format!("{} {}", words::number(iid), m.title),
        format!("{} · {} → {}", m.author, m.source, m.target),
    ];
    if let Some((glyph, word, _)) = &verdict {
        tip.push(format!("pipeline {glyph} {word}"));
    }
    if let Some(n) = threads {
        tip.push(plural(n, "thread"));
    }
    if let Some((text, _)) = who.as_ref().filter(|_| waiting.is_some_and(|w| w > 0)) {
        tip.push(text.clone());
    }
    tip.push(format!("updated {}", hover::full_date(m.updated)));
    let mut meta: Vec<AnyElement> = Vec::new();
    if let Some((text, color)) = who.filter(|_| room.second()) {
        meta.push(div().text_color(color).child(text).into_any_element());
    }
    if let Some((glyph, word, color)) = verdict {
        meta.push(
            div()
                .text_color(color)
                .child(format!("{glyph} {word}"))
                .into_any_element(),
        );
    }
    if let Some(n) = threads.filter(|_| room.third()) {
        meta.push(div().child(plural(n, "thread")).into_any_element());
    }
    let mut line2: Vec<AnyElement> = Vec::new();
    for (n, part) in meta.into_iter().enumerate() {
        if n > 0 {
            line2.push(
                div()
                    .text_color(theme::faint())
                    .child("·")
                    .into_any_element(),
            );
        }
        line2.push(part);
    }
    let compact = look.compact;
    let (context, acts) = (acts.clone(), acts.clone());
    div().px(px(6.)).py(px(1.)).child(
        div()
            .id(("mr-row", iid))
            .on_mouse_down(
                gpui::MouseButton::Right,
                move |e: &gpui::MouseDownEvent, _, cx| {
                    context(
                        Act::RequestContext {
                            iid,
                            at: e.position,
                        },
                        cx,
                    )
                },
            )
            .px(px(10.))
            .py(px(8.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(ROW_RADIUS))
            .cursor_pointer()
            .when(picked, |s| s.bg(theme::selected()))
            .when(!picked, |s| s.hover(|s| s.bg(theme::hover())))
            .child(unread_dot(unread))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(words::number(iid)),
                            )
                            .child(div().flex_1().min_w_0().cut_in_tip().child(m.title.clone()))
                            .children(
                                part.filter(|_| room.third())
                                    .map(|(done, total)| progress_badge(done, total)),
                            )
                            .children(room.second().then(|| {
                                div()
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(format::ago(m.updated))
                            })),
                    )
                    .when(!line2.is_empty(), |s| {
                        s.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(px(12.))
                                .text_color(theme::muted())
                                .children(line2),
                        )
                    }),
            )
            .tip(tip.join("\n"))
            .on_click(move |event: &ClickEvent, _, cx| {
                // A second click on the picked one opens it, as ↵ does; with no preview beside the
                // list, the first (#80).
                if (picked && event.click_count() > 1) || compact {
                    acts(Act::OpenRequest { iid, note: None }, cx)
                } else {
                    acts(Act::PickRequest(iid), cx)
                }
            }),
    )
}

/// The request picked: who and where, its title and description, how far your review is, and
/// Open; for one of yours, the discussions waiting for you first.
fn preview(m: &Mr, rq: &Requests, look: &ListLook, acts: &Acts) -> AnyElement {
    let iid = m.iid;
    let me = rq.login.as_deref().unwrap_or_default();
    let mine = !me.is_empty() && m.author_login == me;
    let preview = rq.rows.preview(iid);
    let mut pills: Vec<AnyElement> = Vec::new();
    if let Some(p) = preview.and_then(|p| p.pipeline.as_ref()) {
        let (glyph, word, color) = crate::bar::pipeline_word(p);
        pills.push(pill(format!("{glyph} {word}"), color).into_any_element());
    }
    if let Some(a) = preview.and_then(|p| p.approvals.as_ref()) {
        let said = if a.required > 0 {
            Some(format!(
                "{}/{} approved",
                a.required - a.left.min(a.required),
                a.required
            ))
        } else {
            (!a.by.is_empty()).then(|| format!("{} approved", a.by.len()))
        };
        pills.extend(said.map(|s| {
            pill(
                s,
                if a.left == 0 && !a.by.is_empty() {
                    theme::added()
                } else {
                    theme::muted()
                },
            )
            .into_any_element()
        }));
    }
    let props = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            div()
                .font_family(theme::code_font())
                .text_color(theme::accent())
                .child(words::number(iid)),
        )
        .child(discussion::avatar(&m.author, mine, 18.))
        .child(
            div()
                .flex()
                .gap(px(6.))
                .child(if mine {
                    "yours".to_owned()
                } else {
                    m.author.clone()
                })
                .child(div().text_color(theme::faint()).child("·"))
                .child(
                    div()
                        .font_family(theme::code_font())
                        .child(format!("{} → {}", m.source, m.target)),
                ),
        )
        .child(div().flex_1())
        .children(pills);
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(m.title.clone());
    let description = markdown::plain(&m.description);
    let description = description.trim();
    // How far your review is, and what is open.
    let mut facts: Vec<String> = Vec::new();
    match (look.seen(iid), look.parts.get(&iid).copied()) {
        (Seen::Done, _) => facts.push("Reviewed at this push".to_owned()),
        (Seen::Earlier, _) => facts.push(match preview.and_then(|p| p.since) {
            Some((viewed, 0)) => format!("Pushed since you viewed v{viewed}, no file changed"),
            Some((viewed, n)) => {
                format!("{} changed since you viewed v{viewed}", plural(n, "file"))
            }
            None => "Pushed since your review".to_owned(),
        }),
        (_, Some((done, total))) => facts.push(format!("{done} of {total} files reviewed")),
        (Seen::Started | Seen::Never, None) => {}
    }
    let threads = (look.threads)(iid);
    if let Some(threads) = threads {
        let open = threads
            .iter()
            .filter(|t| t.resolvable && !t.resolved)
            .count();
        if open > 0 {
            facts.push(format!("{} open", plural(open, "discussion")));
        }
    }
    if m.draft {
        facts.push("Draft".to_owned());
    }
    // A merged or closed one: when it was (its last update, as the list gives it, #114).
    facts.push(match m.state {
        MrState::Opened => format!("updated {}", format::ago(m.updated)),
        s => format!("{} {}", s.word().to_lowercase(), format::ago(m.updated)),
    });
    let waiting = if mine {
        rq.waiting_for_me(threads.unwrap_or_default())
    } else {
        Vec::new()
    };
    let first = waiting.first().and_then(|t| t.notes.last().map(|n| n.id));
    let open_label = if first.is_some() {
        "Open the first"
    } else {
        "Open"
    };
    let open_hint = if first.is_some() {
        words::say("opens the merge request at that discussion").into_owned()
    } else {
        "opens on its changes, at the first file not reviewed · ⌥↓ the next not reviewed".to_owned()
    };
    let waits = (!waiting.is_empty()).then(|| {
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .pb(px(6.))
                    .text_size(px(11.))
                    .text_color(theme::faint())
                    .child(format!("Waiting for you · {}", waiting.len())),
            )
            .children(waiting.iter().enumerate().map(|(n, t)| {
                let last = t.notes.last();
                let who = last.map(|n| n.author.clone()).unwrap_or_default();
                let gist = last
                    .map(|n| format::one_line(&n.body, look.rich))
                    .unwrap_or_default();
                let at = match (&t.path, t.new_line.or(t.old_line)) {
                    (Some(path), Some(line)) => {
                        format!("{}:{line}", path.rsplit('/').next().unwrap_or(path))
                    }
                    (Some(path), None) => path.rsplit('/').next().unwrap_or(path).to_owned(),
                    _ => "General".to_owned(),
                };
                let note = last.map(|n| n.id);
                let acts = acts.clone();
                div()
                    .id(("waiting", n))
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(theme::island_border())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover()))
                    .child(discussion::avatar(&who, false, 18.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .text_size(px(12.))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(6.))
                                    .child(div().font_weight(FontWeight::SEMIBOLD).child(who))
                                    .child(
                                        div().text_color(theme::faint()).child(format!("on {at}")),
                                    ),
                            )
                            .child(
                                hover::cut(SharedString::from(format!("waiting-text-{n}")), gist)
                                    .text_color(theme::muted()),
                            ),
                    )
                    .on_click(move |_: &ClickEvent, _, cx| acts(Act::OpenRequest { iid, note }, cx))
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
        .children(waits)
        .children((!description.is_empty()).then(|| {
            hover::cut_to(
                SharedString::from(format!("mr-description-{iid}")),
                description.to_owned(),
                5,
            )
            .text_size(px(13.))
            .line_height(px(20.))
            .text_color(theme::muted())
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
        .child(
            div().flex().items_center().gap(px(12.)).child(
                primary_button("request-open", open_label, "↵")
                    .tip(open_hint)
                    .on_click(move |_: &ClickEvent, _, cx| {
                        acts(Act::OpenRequest { iid, note: first }, cx)
                    }),
            ),
        )
        .into_any_element()
}

/// Over a list whose host cannot be read now: offline (the VPN), or the token refused; the last
/// list stays under it.
pub fn banner(rq: &Requests, acts: &Acts) -> Option<AnyElement> {
    let error = rq.error.as_ref()?;
    let offline = error.starts_with("Can't reach");
    let since = rq
        .listed_at
        .map(|t| format!(" The list is from {}", format::clock(t)));
    let text = match (offline, &since) {
        (true, Some(since)) => format!("{error}{since} and refreshes when it is back."),
        (true, None) => format!("{error} The list comes when it is back."),
        (false, Some(since)) => format!("{error}.{since}."),
        (false, None) => format!("{error}."),
    };
    let token = error.contains("token");
    let color = if offline {
        theme::accent()
    } else {
        theme::removed()
    };
    let acts = acts.clone();
    Some(
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(14.))
            .py(px(8.))
            .rounded(px(if flat() { 0. } else { ISLAND_RADIUS }))
            .border_1()
            .border_color(color)
            .bg(theme::panel())
            .text_size(px(13.))
            .child(
                icons::icon(if offline { "info" } else { "alert" })
                    .size(px(16.))
                    .text_color(color),
            )
            .child(div().flex_1().min_w_0().child(text))
            .child(
                div()
                    .id("banner-action")
                    .child(outline_button(if token {
                        "Renew token…"
                    } else {
                        "Retry"
                    }))
                    .on_click(move |_: &ClickEvent, _, cx| {
                        acts(if token { Act::RenewToken } else { Act::Relist }, cx)
                    }),
            )
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_is_grouped_by_what_it_needs_from_you() {
        assert_eq!(group(true, true, Seen::Done), Group::Yours);
        assert_eq!(group(false, true, Seen::Never), Group::Asks);
        assert_eq!(group(false, false, Seen::Never), Group::New);
        assert_eq!(group(false, false, Seen::Started), Group::Asks);
        assert_eq!(group(false, true, Seen::Earlier), Group::Updated);
        assert_eq!(group(false, true, Seen::Done), Group::Reviewed);
        // What changed, then what is new, then what is reviewed; yours last.
        assert!(Group::Asks < Group::Updated && Group::Updated < Group::New);
        assert!(Group::New < Group::Reviewed && Group::Reviewed < Group::Yours);
    }

    #[test]
    fn a_projects_folds_and_opened_requests_are_kept_on_its_own_line() {
        let folded = HashSet::from([Group::New, Group::Reviewed]);
        let opened = HashSet::from([415, 398]);
        let text = with_kept_list("b\tasks\t7\tfolds\n", "a", &folded, &opened, true);
        assert_eq!(
            text,
            "a\tothers,reviewed\t398,415\tfolds\nb\tasks\t7\tfolds\n"
        );
        assert_eq!(kept_list(&text, "a"), Some((folded, opened, true)));
        assert_eq!(
            kept_list(&text, "b"),
            Some((HashSet::from([Group::Asks]), HashSet::from([7]), true))
        );
        // Every group unfolded by hand, nothing opened: a line still.
        let none = with_kept_list(&text, "a", &HashSet::new(), &HashSet::new(), true);
        assert_eq!(
            kept_list(&none, "a"),
            Some((HashSet::new(), HashSet::new(), true))
        );
        assert_eq!(kept_list(&none, "c"), None);
        // A line from before the folds were yours: its fold was the old default, so none.
        assert_eq!(
            kept_list("a\treviewed\t398\n", "a"),
            Some((HashSet::new(), HashSet::from([398]), false))
        );
    }

    #[test]
    fn a_plain_comment_waits_for_the_author_until_they_write_again() {
        use artoo_hosts_api::Note;
        let note = |login: &str, created: i64| Note {
            id: created as u64,
            author: login.into(),
            login: login.into(),
            body: String::new(),
            created,
        };
        let thread = |notes: Vec<Note>, resolvable: bool, resolved: bool| Thread {
            id: String::new(),
            notes,
            path: None,
            new_line: None,
            old_line: None,
            resolved,
            resolvable,
            resolved_by: None,
            resolved_at: 0,
            commit: None,
            outdated: false,
        };
        let threads = [
            // An open discussion, its last word theirs: waits.
            thread(vec![note("me", 1), note("ember", 2)], true, false),
            // Resolved: does not.
            thread(vec![note("ember", 3)], true, true),
            // A plain comment after my last word: waits.
            thread(vec![note("kestrel", 9)], false, false),
            // A plain comment before it: I have written since.
            thread(vec![note("kestrel", 4)], false, false),
            // My own last word.
            thread(vec![note("ember", 5), note("me", 8)], true, false),
        ];
        let waits: Vec<i64> = waiting("me", &threads)
            .iter()
            .map(|t| t.notes.last().unwrap().created)
            .collect();
        assert_eq!(waits, [2, 9]);
        assert!(waiting("", &threads).is_empty());
    }

    #[test]
    fn the_band_above_the_lists_top_is_held_until_the_next_pushes_it_up() {
        // Bands at 0, 200 and 300; the list's top at 0: nothing is above it.
        assert_eq!(held_band(&[0., 200., 300.], 0.), None);
        assert_eq!(held_band(&[0., 200., 300.], 100.), Some((0, 0.)));
        // The next band 10 px below the top: the held one goes up by what it covers.
        assert_eq!(held_band(&[0., 200., 300.], 190.), Some((0, -18.)));
        assert_eq!(held_band(&[0., 200., 300.], 250.), Some((1, 0.)));
        assert_eq!(held_band(&[0., 200., 300.], 900.), Some((2, 0.)));
    }
}
