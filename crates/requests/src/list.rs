//! The requests list (#72, #81, #189): grouped by whose turn each request is (or its author, or not at all, as
//! `Display ▾` says), a group's band folding its rows; the folds, the display and when you last read each request
//! kept per project; every row's pipeline, approvals, discussions, pushes and size read in the background, the
//! picked one's merge state and what changed since your review for its preview. Its views draw the rows under their
//! bands, the preview, why none is listed and the banner while the list cannot be read; the tab puts them in the
//! root's frame (`ListLook`, `Body`).

use crate::group::{Band, Group, GroupBy, SortBy};
use crate::requests::{Act, Acts, Requests, Scope};
use crate::turn::{self, Turn};
use artoo_core::git::{DiffSettings, Repo};
use artoo_core::{markdown, rank, storage};
use artoo_hosts_api::{Approvals, Host, MergeState, Mr, MrState, Pipeline, Thread};
use artoo_review_api::Seen;
use artoo_ui_kit::controls::{empty_state, outline_button, primary_button, unread_dot};
use artoo_ui_kit::format::{self, plural};
use artoo_ui_kit::hover::{self, CutInTip, Hover, RowTip};
use artoo_ui_kit::room::Widths;
use artoo_ui_kit::{ISLAND_RADIUS, ROW_RADIUS, discussion, flat, icons, motion, px, theme, words};
use gpui::{AnyElement, ClickEvent, Context, Div, FontWeight, SharedString, Task, div, prelude::*};
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// What the requests list keeps per project (`<repo>\t<folded groups>\t<requests read>\t<folds>\t<display>`, the
/// first two comma-separated, a request read as `<iid>@<its update then>`; `folds` once a group was folded or
/// unfolded by hand; the display as `group::display_key` writes it) (#81, #189). Every group starts open (the
/// developer, 2026-10-23); a line written before the folds were yours carries the old default, read as nothing
/// folded.
const LIST_FILE: &str = "requests-list.txt";

/// The height of a group's band in the requests list.
pub const BAND: f32 = 28.;

/// A request's size: files, lines added, lines removed, commits.
pub type Size = (usize, usize, usize, usize);

/// What a request's row says, read for every request listed (#189): its pipeline, approvals, discussions, when it
/// was pushed and how large it is, with its `updated` then. Kept between starts (`FACTS_FILE`).
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    updated: i64,
    pub pipeline: Option<Pipeline>,
    pub approvals: Option<Approvals>,
    pub threads: Vec<Thread>,
    pub pushes: Vec<i64>,
    pub size: Option<Size>,
}

/// The picked request's preview, beyond its row: where merging it stands and what changed since your review, with
/// its `updated` then.
pub struct Preview {
    updated: i64,
    pub merge: Option<MergeState>,
    /// Pushed since your review: the version you viewed, and how many files changed since then
    /// (X22).
    pub since: Option<(usize, usize)>,
}

/// The list's rows: their facts and previews, its scroll, the groups folded, how they are displayed and when each
/// request was read.
#[derive(Default)]
pub struct Rows {
    facts: HashMap<u64, Facts>,
    reading: HashSet<u64>,
    /// The requests whose facts are still to read, the next first: the picked one, then as the list shows them.
    queue: VecDeque<u64>,
    /// How many reads go on at once, `LANES` at most.
    workers: usize,
    /// The project's default branch: a row names its target only when it is another (#189).
    pub main_line: Option<String>,
    previews: HashMap<u64, Preview>,
    preview_task: Option<Task<()>>,
    /// The requests list's scroll, and whether the next frame brings the picked row into view
    /// (after ↑ ↓ or esc back to the list).
    pub scroll: gpui::ScrollHandle,
    pub reveal: Cell<bool>,
    /// The request picked, previewed: by a click, by ↑ ↓, or the one open as you came back.
    pub picked: Option<u64>,
    /// Project `kept_of`'s requests list: the groups folded, the requests read and their update then — a row's dot
    /// says something new since (#189).
    folded: HashSet<Group>,
    read: HashMap<u64, i64>,
    /// A group was folded or unfolded by hand: the folds are kept.
    chosen: bool,
    /// `Display ▾`: how the rows are grouped and ordered.
    pub display: (GroupBy, SortBy),
    kept_of: Option<String>,
}

/// A request read before its update was kept: nothing it has is new.
const READ_BEFORE: i64 = i64::MAX;
/// How many requests' facts are read at once: each asks the host about four things (#189).
const LANES: usize = 3;
/// Every project's open requests' facts, by request, kept between starts: a start reads only what was updated since.
const FACTS_FILE: &str = "requests-facts.json";

impl Rows {
    /// Project `repo`'s folds, display and requests read, unless they are the ones held.
    pub fn load(&mut self, repo: &str) {
        if self.kept_of.as_deref() == Some(repo) {
            return;
        }
        let kept = kept_list(&storage::read(LIST_FILE), repo).unwrap_or_default();
        (self.folded, self.read, self.chosen, self.display) =
            (kept.folded, kept.read, kept.chosen, kept.display);
        // Another project: its own requests' facts, as they were last read.
        self.facts = kept_facts(&storage::read(FACTS_FILE), repo);
        self.reading.clear();
        self.queue.clear();
        self.previews.clear();
        self.kept_of = Some(repo.to_owned());
    }

    /// The facts of the requests in `open`, kept for the next start; those of requests no longer open go, other
    /// projects' stay as they were.
    fn keep_facts(&self, open: &HashSet<u64>) {
        if let Some(repo) = &self.kept_of {
            let mine = (self.facts.iter()).filter(|(iid, _)| open.contains(iid));
            storage::save(
                FACTS_FILE,
                with_kept_facts(&storage::read(FACTS_FILE), repo, mine),
            );
        }
    }

    /// The next request whose facts are wanted, taken off the queue: open, not read at its update, not being read.
    fn next_to_read(&mut self, list: &[Mr]) -> Option<Mr> {
        while let Some(iid) = self.queue.pop_front() {
            let Some(m) = list
                .iter()
                .find(|m| m.iid == iid && m.state == MrState::Opened)
            else {
                continue;
            };
            let fresh = self.facts.get(&iid).is_some_and(|f| f.updated == m.updated);
            if fresh || !self.reading.insert(iid) {
                continue;
            }
            return Some(m.clone());
        }
        None
    }

    fn keep(&self) {
        if let Some(repo) = &self.kept_of {
            let text = storage::read(LIST_FILE);
            storage::save(
                LIST_FILE,
                with_kept_list(
                    &text,
                    repo,
                    &Kept {
                        folded: self.folded.clone(),
                        read: self.read.clone(),
                        chosen: self.chosen,
                        display: self.display,
                    },
                ),
            );
        }
    }

    /// Whether group `g`'s rows are folded away; a filter typed (`filtering`) opens every group.
    pub fn folds(&self, g: &Group, filtering: bool) -> bool {
        !filtering && self.folded.contains(g)
    }

    /// Group `g` folded or unfolded by hand.
    pub fn toggle(&mut self, g: Group) {
        self.chosen = true;
        if !self.folded.remove(&g) {
            self.folded.insert(g);
        }
        self.keep();
    }

    /// `Display ▾` changed.
    pub fn set_display(&mut self, group: Option<GroupBy>, sort: Option<SortBy>) {
        self.display = (
            group.unwrap_or(self.display.0),
            sort.unwrap_or(self.display.1),
        );
        self.keep();
    }

    /// Whether request `iid` was opened here (#81).
    pub fn opened(&self, iid: u64) -> bool {
        self.read.contains_key(&iid)
    }

    /// Something new on request `m` since you last read it: it was updated after (#189).
    pub fn news(&self, m: &Mr) -> bool {
        self.read.get(&m.iid).is_some_and(|&at| m.updated > at)
    }

    /// Request `iid`'s facts, once read.
    pub fn facts(&self, iid: u64) -> Option<&Facts> {
        self.facts.get(&iid)
    }

    /// Request `iid`'s merge state and what changed since your review, when they were read.
    pub fn preview(&self, iid: u64) -> Option<&Preview> {
        self.previews.get(&iid)
    }
}

/// The order requests `a` and `b` come in under a band, by `sort`: a draft after the others (#189).
fn order(sort: SortBy, size: &dyn Fn(&Mr) -> usize) -> impl Fn(&Mr) -> (bool, i64) + '_ {
    move |m| {
        let key = match sort {
            SortBy::Updated => -m.updated,
            SortBy::Oldest => m.updated,
            SortBy::Size => -(size(m) as i64),
        };
        (m.draft, key)
    }
}

impl Requests {
    /// The requests the scope and `filter` let through, in the order the list shows them: the
    /// merged or the closed as read, newest first, the host having picked the scope's (#114,
    /// #169); the open under their groups (`group_of`), ordered as `Display ▾` says, a draft last in its group.
    /// Copied: a row is drawn with the tab's context.
    pub fn shown(&self, filter: &str) -> Vec<Mr> {
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
            .filter(|m| filter_lets(m, &terms))
            .collect();
        if !past {
            let size = |m: &Mr| self.size_of(m.iid).map_or(0, |(_, a, r, _)| a + r);
            let order = order(self.rows.display.1, &size);
            shown.sort_by_key(|m| (self.group_of(m), order(m)));
        }
        shown.into_iter().cloned().collect()
    }

    /// The next request to review below the open one, going round the list once: the first whose turn is yours or
    /// someone else's, not one you approved, not yours (#81, #189). Counted from where the open one was when it was
    /// opened. `filter` the list's.
    pub fn next_request(&self, filter: &str) -> Option<Mr> {
        let mut shown = self.shown(filter);
        let walk = self
            .walk
            .clone()
            .filter(|(iid, _)| Some(*iid) == self.current);
        if let Some((walked, was)) = walk
            && self.view == MrState::Opened
        {
            let size = |m: &Mr| self.size_of(m.iid).map_or(0, |(_, a, r, _)| a + r);
            let order = order(self.rows.display.1, &size);
            shown.sort_by_key(|m| {
                let group = if m.iid == walked {
                    was.clone()
                } else {
                    self.group_of(m)
                };
                (group, order(m))
            });
        }
        let at = shown.iter().position(|m| Some(m.iid) == self.current);
        let open: Vec<bool> = shown
            .iter()
            .map(|m| matches!(self.turn_group(m), Group::You | Group::Others))
            .collect();
        next_to_review(&open, at).map(|ix| shown[ix].clone())
    }

    /// Request `m`'s group in the list, as `Display ▾` groups it.
    pub fn group_of(&self, m: &Mr) -> Group {
        match self.rows.display.0 {
            GroupBy::Turn => self.turn_group(m),
            GroupBy::Author => Group::Author(m.author.clone()),
            GroupBy::None => Group::All,
        }
    }

    /// Request `m`'s group by whose turn it is: yours, else as its facts say; until they are read, yours when you
    /// are asked to review it, else someone else's.
    pub fn turn_group(&self, m: &Mr) -> Group {
        let me = self.login.as_deref().unwrap_or_default();
        let mine = !me.is_empty() && m.author_login == me;
        match self.turn_of(m) {
            Some(t) => t.group(mine),
            None if mine => Group::Yours,
            None if !me.is_empty() && m.reviewers.iter().any(|r| r == me) => Group::You,
            None => Group::Others,
        }
    }

    /// Whose turn request `m` is, once its facts are read.
    pub fn turn_of(&self, m: &Mr) -> Option<Turn> {
        let me = self.login.as_deref().unwrap_or_default();
        let f = self.rows.facts(m.iid)?;
        Some(turn::turn(
            me,
            m,
            f.approvals.as_ref(),
            &f.threads,
            &f.pushes,
        ))
    }

    /// Request `iid`'s size, once read.
    pub fn size_of(&self, iid: u64) -> Option<Size> {
        self.rows.facts(iid).and_then(|f| f.size)
    }

    /// Of a request's `threads`, those waiting for you, its author (`waiting`).
    pub fn waiting_for_me<'a>(&self, threads: &'a [Thread]) -> Vec<&'a Thread> {
        turn::waiting_on(self.login.as_deref().unwrap_or_default(), threads)
    }

    /// Request `iid` opened: read as it is now; its row's dot goes until it is updated again (#189). Requests no
    /// longer open are forgotten.
    pub fn mark_opened(&mut self, iid: u64) {
        let updated = self.find(iid).map_or(0, |m| m.updated);
        if self.rows.read.get(&iid).is_some_and(|&at| at >= updated) {
            return;
        }
        if !self.list.is_empty() {
            let list = &self.list;
            self.rows
                .read
                .retain(|i, _| list.iter().any(|m| m.iid == *i));
        }
        self.rows.read.insert(iid, updated);
        self.rows.keep();
    }

    /// Every open request's facts not read at its update yet, read in the background a few at a time, each shown as
    /// it comes (#189): its pipeline, approvals, discussions and pushes from `host`, its size from the clone. The
    /// picked request first, then as the list shows them; those kept from the last start at their update are not
    /// read again.
    pub fn read_facts(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        let mut queue: VecDeque<u64> = {
            let size = |m: &Mr| self.size_of(m.iid).map_or(0, |(_, a, r, _)| a + r);
            let order = order(self.rows.display.1, &size);
            let mut open: Vec<&Mr> = (self.list.iter())
                .filter(|m| m.state == MrState::Opened)
                .filter(|m| (self.rows.facts.get(&m.iid)).is_none_or(|f| f.updated != m.updated))
                .collect();
            open.sort_by_key(|m| (self.group_of(m), order(m)));
            open.iter().map(|m| m.iid).collect()
        };
        if let Some(picked) = self.rows.picked
            && let Some(at) = queue.iter().position(|iid| *iid == picked)
        {
            queue.remove(at);
            queue.push_front(picked);
        }
        self.rows.queue = queue;
        if self.rows.main_line.is_none()
            && let Some(root) = self.root.clone()
        {
            cx.spawn(async move |this, cx| {
                let main = cx
                    .background_executor()
                    .spawn(async move { Repo::open(&root).ok().and_then(|r| r.default_branch()) })
                    .await;
                this.update(cx, |rq, cx| {
                    rq.rows.main_line = main;
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        self.pump_facts(host, cx);
    }

    /// Request `iid`, picked: its facts next, if they are still to read.
    pub fn read_facts_first(&mut self, iid: u64, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        if let Some(at) = self.rows.queue.iter().position(|q| *q == iid) {
            self.rows.queue.remove(at);
            self.rows.queue.push_front(iid);
            self.pump_facts(host, cx);
        }
    }

    /// Reads started until `LANES` go on at once, each taking the queue's next until it is empty.
    fn pump_facts(&mut self, host: Arc<dyn Host>, cx: &mut Context<Self>) {
        while self.rows.workers < LANES && !self.rows.queue.is_empty() {
            self.rows.workers += 1;
            let (host, root) = (host.clone(), self.root.clone());
            cx.spawn(async move |this, cx| {
                loop {
                    let next = this
                        .update(cx, |rq, _| {
                            let list = std::mem::take(&mut rq.list);
                            let next = rq.rows.next_to_read(&list);
                            rq.list = list;
                            next
                        })
                        .ok()
                        .flatten();
                    let Some(m) = next else {
                        break;
                    };
                    let iid = m.iid;
                    let (host, root) = (host.clone(), root.clone());
                    let facts = cx
                        .background_executor()
                        .spawn(async move { facts_of(&*host, root.as_deref(), &m) })
                        .await;
                    let kept = this.update(cx, |rq, cx| {
                        rq.rows.reading.remove(&iid);
                        rq.rows.facts.insert(iid, facts);
                        cx.notify();
                    });
                    if kept.is_err() {
                        return;
                    }
                }
                this.update(cx, |rq, _| {
                    rq.rows.workers -= 1;
                    if rq.rows.workers == 0 {
                        let open = rq.list.iter().map(|m| m.iid).collect();
                        rq.rows.keep_facts(&open);
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Request `m`, picked in the list: where merging it stands, once per update of it; with `viewed` (its head now,
    /// and the one you reviewed it at), how many files changed since.
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
        self.read_facts_first(iid, host.clone(), cx);
        if self
            .rows
            .previews
            .get(&iid)
            .is_some_and(|p| p.updated == updated)
        {
            return;
        }
        self.rows.preview_task = Some(cx.spawn(async move |this, cx| {
            let (merge, since) = cx
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
                    (host.merge_state(iid).ok(), since)
                })
                .await;
            this.update(cx, |rq, cx| {
                rq.rows.previews.insert(
                    iid,
                    Preview {
                        updated,
                        merge,
                        since,
                    },
                );
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Request `m`'s facts, read now: from `host`, and its size from the clone at `root` when its head is there.
fn facts_of(host: &dyn Host, root: Option<&Path>, m: &Mr) -> Facts {
    let size = root.and_then(|root| {
        let repo = Repo::open(root).ok()?;
        let head = git2::Oid::from_str(&m.head)
            .ok()
            .filter(|h| repo.has_commit(*h))?;
        let base = repo
            .resolve(&format!("origin/{}", m.target))
            .or_else(|_| repo.resolve(&m.target))
            .ok()?;
        repo.range_stat(base, head)
            .ok()
            .filter(|(files, ..)| *files > 0)
    });
    Facts {
        updated: m.updated,
        pipeline: host.pipeline(m.iid).ok().flatten(),
        approvals: host.approvals(m.iid).ok(),
        threads: host.threads(m.iid).unwrap_or_default(),
        pushes: host
            .versions(m.iid)
            .unwrap_or_default()
            .iter()
            .map(|p| p.created)
            .collect(),
        size,
    }
}

/// `text` (`FACTS_FILE`) with project `repo`'s facts replaced by `facts`; the other projects' as they were.
fn with_kept_facts<'a>(
    text: &str,
    repo: &str,
    facts: impl Iterator<Item = (&'a u64, &'a Facts)>,
) -> String {
    let mut all: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(text).unwrap_or_default();
    let mine: HashMap<u64, &Facts> = facts.map(|(iid, f)| (*iid, f)).collect();
    if let Ok(v) = serde_json::to_value(&mine) {
        all.insert(repo.to_owned(), v);
    }
    serde_json::Value::Object(all).to_string()
}

/// Project `repo`'s requests' facts in `text` (`FACTS_FILE`), as last read.
fn kept_facts(text: &str, repo: &str) -> HashMap<u64, Facts> {
    serde_json::from_str::<HashMap<String, serde_json::Value>>(text)
        .ok()
        .and_then(|mut all| all.remove(repo))
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// What the list keeps of a project.
#[derive(Default, Debug, PartialEq)]
struct Kept {
    folded: HashSet<Group>,
    read: HashMap<u64, i64>,
    chosen: bool,
    display: (GroupBy, SortBy),
}

/// Repository `repo`'s line of `requests-list.txt`: its folded groups (none unless folded by hand), the requests
/// read, whether the folds are yours and the display; none when it has no line.
fn kept_list(text: &str, repo: &str) -> Option<Kept> {
    let line = text.lines().find(|l| l.split('\t').next() == Some(repo))?;
    let parts: Vec<&str> = line.split('\t').skip(1).collect();
    let field = |n: usize| {
        parts
            .get(n)
            .copied()
            .unwrap_or_default()
            .split(',')
            .filter(|f| !f.is_empty())
    };
    let chosen = parts.get(2) == Some(&"folds");
    let folded = field(0)
        .filter_map(crate::group::parse)
        .filter(|_| chosen)
        .collect();
    // Read before its update was kept: nothing it has is new.
    let read = field(1)
        .filter_map(|r| match r.split_once('@') {
            Some((iid, at)) => Some((iid.parse().ok()?, at.parse().ok()?)),
            None => Some((r.parse().ok()?, READ_BEFORE)),
        })
        .collect();
    let display = parts
        .get(3)
        .map_or_else(Default::default, |d| crate::group::parse_display(d));
    Some(Kept {
        folded,
        read,
        chosen,
        display,
    })
}

/// `text` with repository `repo`'s line written anew; other repositories' kept.
fn with_kept_list(text: &str, repo: &str, kept: &Kept) -> String {
    let mut folded: Vec<String> = kept.folded.iter().map(Band::key).collect();
    folded.sort();
    let mut read: Vec<(u64, i64)> = kept.read.iter().map(|(&i, &a)| (i, a)).collect();
    read.sort();
    let line = format!(
        "{repo}\t{}\t{}\t{}\t{}",
        folded.join(","),
        read.iter()
            .map(|(iid, at)| if *at == READ_BEFORE {
                iid.to_string()
            } else {
                format!("{iid}@{at}")
            })
            .collect::<Vec<_>>()
            .join(","),
        if kept.chosen { "folds" } else { "" },
        crate::group::display_key(kept.display.0, kept.display.1),
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
        Some(_) => vec![Group::All; shown.len()],
        None => shown.iter().map(|m| rq.group_of(m)).collect(),
    };
    // Not grouped (`Display ▾`): one list, no band, nothing to fold.
    let folds = |g: &Group| *g != Group::All && rq.rows.folds(g, look.filtering());
    let picked = look
        .picked
        .and_then(|iid| shown.iter().find(|m| m.iid == iid))
        .or_else(|| {
            shown
                .iter()
                .zip(&groups)
                .find(|(_, g)| past.is_some() || !folds(g))
                .map(|(m, _)| m)
        });
    // Each group a band, its rows under it unless it is folded (#81).
    let mut rows: Vec<AnyElement> = Vec::new();
    let mut bands: Vec<(usize, Group, usize)> = Vec::new();
    let mut picked_row = None;
    for (n, (m, g)) in shown.iter().zip(&groups).enumerate() {
        if past.is_some() || *g == Group::All {
            if picked.is_some_and(|p| p.iid == m.iid) {
                picked_row = Some(rows.len());
            }
            rows.push(row(m, rq, look, acts).into_any_element());
            continue;
        }
        if n == 0 || groups[n - 1] != *g {
            let count = groups.iter().filter(|o| *o == g).count();
            bands.push((rows.len(), g.clone(), count));
            let top = rows.is_empty();
            rows.push(band(g.clone(), count, false, top, rq, look, acts).into_any_element());
        }
        let is_picked = picked.is_some_and(|p| p.iid == m.iid);
        // Each row folds with its group, in size and alpha (#110): the group opens and shuts as
        // one, and every row stays a child of its own for the held band.
        let Some(row) = (look.fold)(
            format!("request-row-{}", m.iid),
            &format!("group-{}", g.key()),
            !folds(g) || is_picked,
            row(m, rq, look, acts).into_any_element(),
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
            let (_, g, count) = bands[at].clone();
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
            "Looks in number, title, author and branch.".to_owned(),
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
                words::say(&format!("Couldn't read the {word} merge requests")).into_owned(),
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
        let (title, more) = match rq.scope {
            Scope::All => (
                words::say(&format!("No {word} merge requests")).into_owned(),
                false,
            ),
            Scope::ReviewMe => (format!("None {word} asked for your review"), read),
            Scope::Mine => (format!("None of yours is {word}"), read),
        };
        return empty_state(
            "pull-request",
            title,
            String::new(),
            more.then(|| show_all("Show all")),
        );
    }
    // A read that failed is never "none" (#185): the banner over the list says why, with Retry.
    if rq.error.is_some() && all == 0 {
        return empty_state(
            "pull-request",
            words::say("Couldn't read the merge requests").into_owned(),
            String::new(),
            None,
        );
    }
    // A state word and its way on, no sentence (#185).
    let (title, more) = match rq.scope {
        Scope::ReviewMe => ("Nothing to review".to_owned(), all > 0),
        Scope::Mine => ("None of yours is open".to_owned(), all > 0),
        Scope::All => (words::say("No open merge requests").into_owned(), false),
    };
    empty_state(
        "pull-request",
        title,
        String::new(),
        more.then(|| show_all("Show all")),
    )
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
    let folded = rq.rows.folds(&g, look.filtering());
    let acts = acts.clone();
    div()
        .id(SharedString::from(format!(
            "{}-{}",
            if held { "group-held" } else { "group-band" },
            g.key()
        )))
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
                            acts(Act::ToggleGroup(g.clone()), cx)
                        }
                    },
                )
        })
}

/// The colour whose turn a request is said in: yours the accent, approved green, someone else's muted.
fn turn_color(t: &Turn) -> gpui::Rgba {
    match t {
        Turn::You { .. } => theme::accent(),
        Turn::Approved => theme::added(),
        Turn::Them { .. } => theme::muted(),
    }
}

/// A small outlined tag after a title: `New`, `Draft`.
fn tag(text: &'static str, color: gpui::Rgba) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .h(px(18.))
        .flex()
        .items_center()
        .rounded_full()
        .border_1()
        .border_color(color)
        .text_size(px(11.))
        .text_color(color)
        .child(text)
}

/// A part of a row's second line.
enum Part {
    Text(String, gpui::Rgba),
    /// The number or the author: muted, the filter's hits marked (#191).
    Found(String),
    /// Files, lines added, lines removed.
    Size(usize, usize, usize),
}

/// Whether a request passes the list's filter, typed as `terms` (lower case): each matches its number, title,
/// author (name or username) or source branch (#191).
pub(crate) fn filter_lets(m: &Mr, terms: &[&str]) -> bool {
    let hay = format!(
        "{} {} {} {} {}",
        words::number(m.iid),
        m.title,
        m.author,
        m.author_login,
        m.source
    )
    .to_lowercase();
    terms.iter().all(|w| rank::word_matches(w, &hay))
}

/// Where the filter's words are found in `text`, in order and apart: the stretches a row marks (#191). A word
/// matched by its initials only is not marked.
fn hits(text: &str, filter: &str) -> Vec<std::ops::Range<usize>> {
    let lower = text.to_lowercase();
    // A text whose lower case changes its length is not marked.
    if lower.len() != text.len() {
        return Vec::new();
    }
    let mut found: Vec<std::ops::Range<usize>> = filter
        .to_lowercase()
        .split_whitespace()
        .flat_map(|w| {
            lower
                .match_indices(w)
                .map(|(at, w)| at..at + w.len())
                .collect::<Vec<_>>()
        })
        .collect();
    found.sort_by_key(|r| r.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::new();
    for r in found {
        match merged.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    merged
}

/// `text` with the filter's hits marked as find marks them.
fn marked(text: String, filter: &str) -> gpui::StyledText {
    let ranges = hits(&text, filter);
    let mark = gpui::HighlightStyle {
        color: Some(theme::text().into()),
        background_color: Some(theme::warning_bg().into()),
        ..Default::default()
    };
    gpui::StyledText::new(text).with_highlights(ranges.into_iter().map(|r| (r, mark)))
}

/// About how wide `text` is drawn at 12 px.
fn estimate(text: &str) -> f32 {
    text.chars().count() as f32 * 6.1
}

impl Part {
    /// About how wide it is drawn.
    fn estimate(&self) -> f32 {
        match self {
            Part::Text(text, _) | Part::Found(text) => estimate(text),
            Part::Size(files, added, removed) => {
                estimate(&format!("{} +{added} −{removed}", plural(*files, "file")))
            }
        }
    }

    /// How much it is worth keeping when the row is narrow: the number, author and pipeline most; a conflict; the
    /// approvals; then the size, the discussions, the target.
    fn keep(&self) -> u8 {
        match self {
            Part::Text(text, _) if text.starts_with('→') => 0,
            Part::Text(text, _) if text.ends_with(" open") => 1,
            Part::Size(..) => 2,
            Part::Text(text, _) if text.starts_with('⚠') => 4,
            _ => 3,
        }
    }

    /// Drawn whole, or — the line's last — cut when the row has no room, saying so.
    fn drawn(self, last: bool, filter: &str) -> AnyElement {
        let cut = |text: String, color| {
            div()
                .min_w_0()
                .cut_in_tip()
                .text_color(color)
                .child(text)
                .into_any_element()
        };
        match self {
            Part::Found(text) if last => div()
                .min_w_0()
                .cut_in_tip()
                .child(marked(text, filter))
                .into_any_element(),
            Part::Found(text) => div()
                .flex_none()
                .child(marked(text, filter))
                .into_any_element(),
            Part::Text(text, color) if last => cut(text, color),
            Part::Text(text, color) => div()
                .flex_none()
                .text_color(color)
                .child(text)
                .into_any_element(),
            Part::Size(files, added, removed) if last => cut(
                format!("{} +{added} −{removed}", plural(files, "file")),
                theme::muted(),
            ),
            Part::Size(files, added, removed) => div()
                .flex_none()
                .flex()
                .gap(px(4.))
                .child(plural(files, "file"))
                .child(div().text_color(theme::added()).child(format!("+{added}")))
                .child(
                    div()
                        .text_color(theme::removed())
                        .child(format!("−{removed}")),
                )
                .into_any_element(),
        }
    }
}

/// Approvals as a row and the preview say them: `1 of 2`, or how many approved where no rule asks.
fn approvals_words(a: &Approvals) -> Option<String> {
    if a.required > 0 {
        Some(format!(
            "{} of {}",
            a.required - a.left.min(a.required),
            a.required
        ))
    } else {
        (!a.by.is_empty()).then(|| format!("{} approved", a.by.len()))
    }
}

/// Of a request's `threads`, those open (resolvable, not resolved) and those resolved.
fn discussions(threads: &[Thread]) -> (usize, usize) {
    let resolvable = threads.iter().filter(|t| t.resolvable);
    let open = resolvable.clone().filter(|t| !t.resolved).count();
    (open, resolvable.count() - open)
}

/// A request's row (#189, Design L1): its title — muted for a draft, tagged `New` or `Draft` — and when; under it
/// its number, author, pipeline, approvals, size and open discussions, a conflict, a target that is not the
/// default branch; at its end whose turn it is. The dot: something new since you read it.
fn row(m: &Mr, rq: &Requests, look: &ListLook, acts: &Acts) -> impl IntoElement + use<> {
    let iid = m.iid;
    let picked = look.picked == Some(iid);
    let open = m.state == MrState::Opened;
    let facts = rq.rows.facts(iid);
    let me = rq.login.as_deref().unwrap_or_default();
    let mine = !me.is_empty() && m.author_login == me;
    let new_to_you = open && !opened_before(rq, iid, look.seen(iid));
    let unread = open && !new_to_you && rq.rows.news(m);
    let turn = if open { rq.turn_of(m) } else { None };
    // Its anatomy (#79): the title and when always; the size, discussions and target leave first as the list
    // narrows, then whose turn; the tooltip holds it all.
    let room = look.widths.room("requests");
    let mut tip = vec![
        format!("{} {}", words::number(iid), m.title),
        format!("{} · {} → {}", m.author, m.source, m.target),
    ];
    // Each part whole, but the last, which is cut and says so (#184).
    let mut parts: Vec<Part> = vec![
        Part::Found(words::number(iid)),
        Part::Found(if mine {
            "You".to_owned()
        } else {
            m.author.clone()
        }),
    ];
    if let Some(p) = facts.and_then(|f| f.pipeline.as_ref()) {
        let (glyph, short, color) = crate::bar::pipeline_short(p);
        tip.push(format!(
            "Pipeline {glyph} {}",
            crate::bar::pipeline_word(p).1
        ));
        parts.push(Part::Text(format!("{glyph} {short}"), color));
    }
    if let Some(said) = facts
        .and_then(|f| f.approvals.as_ref())
        .and_then(approvals_words)
    {
        tip.push(format!("Approvals {said}"));
        parts.push(Part::Text(said, theme::muted()));
    }
    if let Some((files, added, removed, _)) = facts.and_then(|f| f.size) {
        tip.push(format!("{} +{added} −{removed}", plural(files, "file")));
        if room.third() {
            parts.push(Part::Size(files, added, removed));
        }
    }
    let (open_threads, _) = facts.map(|f| discussions(&f.threads)).unwrap_or_default();
    if open_threads > 0 {
        tip.push(format!("{open_threads} open"));
        if room.third() {
            parts.push(Part::Text(format!("{open_threads} open"), theme::muted()));
        }
    }
    if m.conflicts {
        tip.push("Conflicts with its target".into());
        parts.push(Part::Text("⚠ conflicts".into(), theme::warning()));
    }
    if rq
        .rows
        .main_line
        .as_deref()
        .is_some_and(|main| main != m.target)
    {
        tip.push(format!("Into {}", m.target));
        if room.third() {
            parts.push(Part::Text(format!("→ {}", m.target), theme::muted()));
        }
    }
    if let Some(t) = &turn {
        tip.push(t.words(mine));
    }
    tip.push(format!("updated {}", hover::full_date(m.updated)));
    // What the row has room for, as the list was last measured: the parts of least use leave whole — the target,
    // the open discussions, the size —, the last one left cut when even the rest does not fit.
    let width = look.widths.width("requests");
    if width > 0. {
        let turn_width = turn.as_ref().map_or(0., |t| estimate(&t.words(mine)) + 8.);
        let budget = width - 52. - turn_width;
        let needs = |parts: &[Part]| {
            parts.iter().map(Part::estimate).sum::<f32>() + 14. * parts.len() as f32
        };
        while parts.len() > 3 && needs(&parts) > budget {
            let least = parts
                .iter()
                .enumerate()
                .skip(3)
                .min_by_key(|(n, p)| (p.keep(), std::cmp::Reverse(*n)))
                .map(|(n, _)| n);
            match least {
                Some(n) => parts.remove(n),
                None => break,
            };
        }
    }
    let last = parts.len().saturating_sub(1);
    let mut line2: Vec<AnyElement> = Vec::new();
    for (n, part) in parts.into_iter().enumerate() {
        if n > 0 {
            line2.push(
                div()
                    .flex_none()
                    .text_color(theme::faint())
                    .child("·")
                    .into_any_element(),
            );
        }
        line2.push(part.drawn(n == last, look.filter));
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
                                    .min_w_0()
                                    .cut_in_tip()
                                    .when(m.draft, |s| s.text_color(theme::muted()))
                                    .child(marked(m.title.clone(), look.filter)),
                            )
                            .when(new_to_you, |s| s.child(tag("New", theme::accent())))
                            .when(m.draft, |s| s.child(tag("Draft", theme::muted())))
                            .child(div().flex_1())
                            .children(room.second().then(|| {
                                div()
                                    .flex_none()
                                    .text_size(px(11.))
                                    .text_color(theme::faint())
                                    .child(format::ago(m.updated))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(5.))
                                    .overflow_hidden()
                                    .text_color(theme::muted())
                                    .children(line2),
                            )
                            .children(turn.filter(|_| room.second()).map(|t| {
                                div()
                                    .flex_none()
                                    .text_color(turn_color(&t))
                                    .child(t.words(mine))
                            })),
                    ),
            )
            .row_tip(picked, tip.join("\n"))
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

/// The request picked (#189, Design L3): its number and branches, whose turn it is; its title; labelled rows —
/// Size, Pipeline, Approvals, Reviewers, In the way, Discussions, Your review —; for one of yours the discussions
/// waiting for you; the description; Open.
fn preview(m: &Mr, rq: &Requests, look: &ListLook, acts: &Acts) -> AnyElement {
    let iid = m.iid;
    let me = rq.login.as_deref().unwrap_or_default();
    let mine = !me.is_empty() && m.author_login == me;
    let open = m.state == MrState::Opened;
    let facts = rq.rows.facts(iid);
    let turn = if open { rq.turn_of(m) } else { None };
    let id_line = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(theme::muted())
        .child(
            div()
                .flex_none()
                .font_family(theme::code_font())
                .text_color(theme::accent())
                .child(words::number(iid)),
        )
        .child(
            hover::branches("preview-branches", &m.source, &m.target)
                .min_w_0()
                .font_family(theme::code_font()),
        )
        .child(div().flex_1())
        .children(turn.as_ref().map(|t| {
            div()
                .flex_none()
                .text_color(turn_color(t))
                .child(t.words(mine))
        }))
        // A merged or closed one: what became of it, and when (#114).
        .children((!open).then(|| {
            div()
                .flex_none()
                .child(format!("{} {}", m.state.word(), format::ago(m.updated)))
        }));
    let title = div()
        .text_size(px(19.))
        .line_height(px(26.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(m.draft, |s| s.text_color(theme::muted()))
        .child(m.title.clone());
    let rule = || div().h(px(1.)).bg(theme::island_border());
    // The labelled rows.
    let mut facts_rows: Vec<(&'static str, AnyElement)> = Vec::new();
    let text = |t: String| div().child(t).into_any_element();
    if let Some((files, added, removed, commits)) = facts.and_then(|f| f.size) {
        facts_rows.push((
            "Size",
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .child(plural(files, "file"))
                .child(div().text_color(theme::faint()).child("·"))
                .child(div().text_color(theme::added()).child(format!("+{added}")))
                .child(
                    div()
                        .text_color(theme::removed())
                        .child(format!("−{removed}")),
                )
                .child(div().text_color(theme::faint()).child("·"))
                .child(plural(commits, "commit"))
                .into_any_element(),
        ));
    }
    if let Some(p) = facts.and_then(|f| f.pipeline.as_ref()) {
        let (glyph, word, color) = crate::bar::pipeline_word(p);
        facts_rows.push((
            "Pipeline",
            div()
                .text_color(color)
                .child(format!("{glyph} {word}"))
                .into_any_element(),
        ));
    }
    let approvals = facts.and_then(|f| f.approvals.as_ref());
    if let Some(a) = approvals {
        let mut said: Vec<String> = approvals_words(a).into_iter().collect();
        said.extend(a.by.iter().map(|by| format!("{by} ✓")));
        if !said.is_empty() {
            facts_rows.push(("Approvals", text(said.join(" · "))));
        }
    }
    let threads: &[Thread] = facts.map_or(&[], |f| f.threads.as_slice());
    if !m.reviewers.is_empty() {
        let name_of = |login: &str| {
            if login == me {
                return "You".to_owned();
            }
            threads
                .iter()
                .flat_map(|t| &t.notes)
                .find(|n| n.login == login)
                .map(|n| n.author.clone())
                .unwrap_or_else(|| login.to_owned())
        };
        let people = m.reviewers.iter().map(|login| {
            let name = name_of(login);
            let approved =
                approvals.is_some_and(|a| a.by.contains(&name) || (login == me && a.mine));
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(discussion::avatar(&name, login == me, 16.))
                .child(name)
                .child(if approved {
                    div().text_color(theme::added()).child("✓ approved")
                } else {
                    div().text_color(theme::faint()).child("reviewing")
                })
        });
        facts_rows.push((
            "Reviewers",
            div()
                .flex()
                .flex_wrap()
                .gap_x(px(14.))
                .gap_y(px(4.))
                .children(people)
                .into_any_element(),
        ));
    }
    let preview = rq.rows.preview(iid);
    if let Some(host) = preview.and_then(|p| p.merge.as_ref()).filter(|_| open) {
        let (open_threads, _) = discussions(threads);
        let stand = crate::merge::stand(
            host,
            &crate::merge::Known {
                draft: m.draft,
                approvals,
                open_threads,
                pipeline: facts.and_then(|f| f.pipeline.as_ref()),
                url: &m.url,
                target: &m.target,
            },
        );
        if let crate::merge::Stand::InTheWay(blocks) = stand {
            facts_rows.push((
                "In the way",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(
                        blocks
                            .iter()
                            .map(|b| div().child(format!("{}: {}", b.what, b.detail))),
                    )
                    .into_any_element(),
            ));
        }
    }
    let (open_threads, resolved) = discussions(threads);
    if open_threads + resolved > 0 {
        facts_rows.push((
            "Discussions",
            text(format!("{open_threads} open · {resolved} resolved")),
        ));
    }
    let review = match (look.seen(iid), look.parts.get(&iid).copied()) {
        (Seen::Done, _) => Some("Reviewed at this push".to_owned()),
        (Seen::Earlier, _) => Some(match preview.and_then(|p| p.since) {
            Some((viewed, 0)) => format!("Pushed since you viewed v{viewed}, no file changed"),
            Some((viewed, n)) => {
                format!("{} changed since you viewed v{viewed}", plural(n, "file"))
            }
            None => "Pushed since your review".to_owned(),
        }),
        (_, Some((done, total))) => Some(format!("{done} of {}", plural(total as usize, "file"))),
        (_, None) => facts
            .and_then(|f| f.size)
            .map(|(files, ..)| format!("0 of {}", plural(files, "file"))),
    };
    if let Some(review) = review.filter(|_| open && !mine) {
        facts_rows.push(("Your review", text(review)));
    }
    let labelled = div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .text_size(px(12.))
        .children(facts_rows.into_iter().map(|(label, value)| {
            div()
                .flex()
                .items_start()
                .gap(px(12.))
                .child(
                    div()
                        .w(px(108.))
                        .flex_none()
                        .text_color(theme::muted())
                        .child(label),
                )
                .child(div().flex_1().min_w_0().child(value))
        }));
    let description = markdown::plain(&m.description);
    let description = description.trim();
    let waiting = if mine {
        rq.waiting_for_me(threads)
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
        .child(id_line)
        .child(title)
        .child(rule())
        .child(labelled)
        .child(rule())
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
    // Facts on the banner, `Can't reach gitlab.example.com · list from 14:32`; the guess after the dash (the VPN)
    // and the whole date in its tip (#185).
    let fact = error.split_once(" — ").map_or(error.as_str(), |(f, _)| f);
    let fact = fact.trim_end_matches('.');
    let text = match rq.listed_at {
        Some(t) => format!("{fact} · list from {}", format::clock(t)),
        None => fact.to_owned(),
    };
    let mut tip = vec![error.clone()];
    if let Some(t) = rq.listed_at {
        tip.push(format!("List from {}", hover::full_date(t)));
    }
    let token = error.contains("token");
    let color = if offline {
        theme::accent()
    } else {
        theme::removed()
    };
    let acts = acts.clone();
    Some(
        div()
            .id("requests-banner")
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
            .tip(tip.join("\n"))
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

/// The row to review after row `at`, going round: the nearest of the lowest tier; a row of no
/// tier is never one.
fn next_to_review(open: &[bool], at: Option<usize>) -> Option<usize> {
    let n = open.len();
    let start = at.map_or(0, |a| a + 1);
    (0..n)
        .map(|step| (start + step) % n)
        .find(|&ix| Some(ix) != at && open[ix])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_projects_folds_reads_and_display_are_kept_on_its_own_line() {
        let kept = Kept {
            folded: HashSet::from([Group::Others, Group::Done]),
            read: HashMap::from([(415, 1_700), (398, READ_BEFORE)]),
            chosen: true,
            display: (GroupBy::Author, SortBy::Oldest),
        };
        let text = with_kept_list("b\tyou\t7\tfolds\n", "a", &kept);
        assert_eq!(
            text,
            "a\tdone,others\t398,415@1700\tfolds\tauthor/oldest\nb\tyou\t7\tfolds\n"
        );
        assert_eq!(kept_list(&text, "a"), Some(kept));
        // A line from before the display: the defaults; a request read before its update was kept is not new.
        let b = kept_list(&text, "b").unwrap();
        assert_eq!(b.folded, HashSet::from([Group::You]));
        assert_eq!(b.read, HashMap::from([(7, READ_BEFORE)]));
        assert_eq!(b.display, (GroupBy::Turn, SortBy::Updated));
        assert_eq!(kept_list(&text, "c"), None);
        // A line from before the folds were yours: its fold was the old default, so none.
        assert_eq!(
            kept_list("a\treviewed\t398\n", "a").map(|k| k.folded),
            Some(HashSet::new())
        );
    }

    #[test]
    fn facts_are_kept_per_project_and_only_the_wanted_are_read() {
        let facts = |updated: i64, pushes: Vec<i64>| Facts {
            updated,
            pushes,
            ..Default::default()
        };
        let a = HashMap::from([(7, facts(70, vec![1, 2])), (8, facts(80, vec![]))]);
        let text = with_kept_facts("", "a", a.iter());
        let text = with_kept_facts(&text, "b", [(&9, &facts(90, vec![3]))].into_iter());
        // Project a's again, 8 no longer open: b's stay.
        let text = with_kept_facts(&text, "a", a.iter().filter(|(iid, _)| **iid == 7));
        let kept = kept_facts(&text, "a");
        assert_eq!(kept.keys().collect::<Vec<_>>(), [&7]);
        assert_eq!(
            (kept[&7].updated, kept[&7].pushes.clone()),
            (70, vec![1, 2])
        );
        assert_eq!(kept_facts(&text, "b")[&9].updated, 90);
        assert!(kept_facts(&text, "c").is_empty() && kept_facts("not json", "a").is_empty());

        // The queue's next: open, not read at its update, not being read.
        let m = |iid: u64, updated: i64, state: MrState| Mr {
            iid,
            updated,
            state,
            ..Default::default()
        };
        let list = [
            m(7, 70, MrState::Opened),
            m(8, 81, MrState::Opened),
            m(9, 90, MrState::Merged),
            m(10, 100, MrState::Opened),
            m(11, 110, MrState::Opened),
        ];
        let mut rows = Rows {
            facts: HashMap::from([(7, facts(70, vec![])), (8, facts(80, vec![]))]),
            queue: VecDeque::from([7, 9, 11, 8, 10, 12]),
            ..Default::default()
        };
        rows.reading.insert(11);
        let read: Vec<u64> =
            std::iter::from_fn(|| rows.next_to_read(&list).map(|m| m.iid)).collect();
        assert_eq!(read, [8, 10]);
        assert!(rows.reading.contains(&8) && rows.reading.contains(&10));
    }

    #[test]
    fn a_draft_comes_last_and_the_sort_orders_the_rest() {
        let m = |iid: u64, updated: i64, draft: bool| Mr {
            iid,
            updated,
            draft,
            ..Default::default()
        };
        let mut list = [m(1, 10, false), m(2, 30, true), m(3, 20, false)];
        let size = |m: &Mr| m.iid as usize;
        list.sort_by_key(order(SortBy::Updated, &size));
        assert_eq!(list.iter().map(|m| m.iid).collect::<Vec<_>>(), [3, 1, 2]);
        list.sort_by_key(order(SortBy::Oldest, &size));
        assert_eq!(list.iter().map(|m| m.iid).collect::<Vec<_>>(), [1, 3, 2]);
        list.sort_by_key(order(SortBy::Size, &size));
        assert_eq!(list.iter().map(|m| m.iid).collect::<Vec<_>>(), [3, 1, 2]);
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
        let waits: Vec<i64> = crate::turn::waiting_on("me", &threads)
            .iter()
            .map(|t| t.notes.last().unwrap().created)
            .collect();
        assert_eq!(waits, [2, 9]);
        assert!(crate::turn::waiting_on("", &threads).is_empty());
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

    #[test]
    fn the_next_request_to_review_goes_round_to_the_top() {
        // 0 to review, 1 reviewed, 2 current, 3 reviewed: past the end, back to 0.
        assert_eq!(
            next_to_review(&[true, false, true, false], Some(2)),
            Some(0)
        );
        assert_eq!(next_to_review(&[false, true, false], Some(0)), Some(1));
        assert_eq!(next_to_review(&[false, true], None), Some(1));
        // Only the current one is left: nowhere to go.
        assert_eq!(next_to_review(&[false, true, false], Some(1)), None);
        assert_eq!(next_to_review(&[false, false, false], Some(1)), None);
    }

    #[test]
    fn the_one_below_comes_before_the_top() {
        // 0 needs review, 1 current, 2 reviewed, 3 new to you: on down to 3, not back up to 0
        // (#136).
        assert_eq!(next_to_review(&[true, true, false, true], Some(1)), Some(3));
    }

    #[test]
    fn the_filter_finds_the_author_by_username_and_marks_where_it_matched() {
        let m = Mr {
            iid: 398,
            title: "Refactor the login screen".into(),
            author: "Juniper Hale".into(),
            author_login: "jhale".into(),
            source: "feature/login-state".into(),
            ..Default::default()
        };
        assert!(filter_lets(&m, &["jhale"]));
        assert!(filter_lets(&m, &["juniper", "login"]));
        assert!(!filter_lets(&m, &["ember"]));
        // Each word's hits, in order, those that touch made one.
        assert_eq!(
            hits("Refactor the login screen", "LOG the"),
            vec![9..12, 13..16]
        );
        assert_eq!(hits("aaa", "aa a"), vec![0..3]);
        assert!(hits("Juniper Hale", "").is_empty());
    }
}
